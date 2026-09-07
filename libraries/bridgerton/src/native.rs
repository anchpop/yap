//! Native ABI. Unsafe pointer operations are confined here; every access checks the main thread.
//!
//! Handles represent one owned Rc reference. Generated callers must pass valid, correctly typed
//! handles, transfer ownership exactly once, and free them on the main thread. Cloning for each
//! call keeps the object alive even when a host callback releases its last external reference.
//! No object or future is reachable from a Waker: only the thread-safe Signal is shared.

use crate::{Callback, Error};
use std::{
    ffi::c_void,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{Arc, Mutex},
    task::{Context, Poll, Wake, Waker},
};

pub const OK: u32 = 0;
pub const PENDING: u32 = 1;
pub const FAILED: u32 = 2;
pub const TYPED_ERROR: u32 = 3;

#[doc(hidden)]
pub fn typed_error_bytes(bytes: Vec<u8>) -> BridgeResult {
    BridgeResult {
        data: Buffer::new(bytes),
        ..BridgeResult::empty(TYPED_ERROR)
    }
}

#[repr(C)]
pub struct Buffer {
    pub data: *mut u8,
    pub len: usize,
}

impl Buffer {
    fn empty() -> Self {
        Self {
            data: std::ptr::null_mut(),
            len: 0,
        }
    }
    fn new(value: impl Into<Vec<u8>>) -> Self {
        let bytes = value.into().into_boxed_slice();
        let len = bytes.len();
        Self {
            data: Box::into_raw(bytes).cast(),
            len,
        }
    }
}

#[repr(C)]
pub struct BridgeResult {
    pub handle: *const c_void,
    pub value: u32,
    pub status: u32,
    pub data: Buffer,
}

/// Borrowed bytes, valid only for the duration of a generated C call.
#[repr(C)]
pub struct Bytes {
    pub data: *const u8,
    pub len: usize,
}

/// # Safety
/// Nonempty input must point to `len` readable bytes throughout this call.
pub unsafe fn read_value<T: crate::value::Value>(bytes: Bytes) -> Result<T, Error> {
    require_main_thread();
    if bytes.len > crate::value::MAX_BYTES {
        return Err(Error::new("value exceeds byte limit"));
    }
    let slice = if bytes.len == 0 {
        &[]
    } else {
        if bytes.data.is_null() {
            return Err(Error::new("null value bytes"));
        }
        unsafe { std::slice::from_raw_parts(bytes.data, bytes.len) }
    };
    crate::value::decode(slice)
}

pub fn value<T: crate::value::Value>(value: &T) -> Result<BridgeResult, Error> {
    Ok(BridgeResult {
        data: Buffer::new(crate::value::encode(value)?),
        ..BridgeResult::empty(OK)
    })
}

impl BridgeResult {
    fn empty(status: u32) -> Self {
        Self {
            handle: std::ptr::null(),
            value: 0,
            status,
            data: Buffer::empty(),
        }
    }
    fn error(status: u32, message: String) -> Self {
        Self {
            data: Buffer::new(message),
            ..Self::empty(status)
        }
    }
}

mod bindings;
mod callbacks;
mod returns;
pub use bindings::{
    NativeArgument, NativeBorrowed, NativeError, NativeObject, NativeOptionalArgument,
    NativeReturn, return_method,
};
pub use callbacks::{CallbackArguments, callback_arguments};

pub trait IntoResult {
    fn into_result(self) -> BridgeResult;
}
impl IntoResult for u32 {
    fn into_result(self) -> BridgeResult {
        BridgeResult {
            value: self,
            ..BridgeResult::empty(OK)
        }
    }
}
impl IntoResult for () {
    fn into_result(self) -> BridgeResult {
        BridgeResult::empty(OK)
    }
}
impl IntoResult for String {
    fn into_result(self) -> BridgeResult {
        BridgeResult {
            data: Buffer::new(self),
            ..BridgeResult::empty(OK)
        }
    }
}

#[cfg(target_vendor = "apple")]
unsafe extern "C" {
    fn pthread_main_np() -> std::ffi::c_int;
}

/// An unconditional check before dereferencing any confined pointer, including on free.
pub fn require_main_thread() {
    #[cfg(target_vendor = "apple")]
    let is_main = unsafe { pthread_main_np() != 0 };
    #[cfg(not(target_vendor = "apple"))]
    let is_main = false; // Non-Apple builds can use Rust APIs/generation, but not this Swift runtime.

    if !is_main {
        eprintln!("bridgerton: attempted native access outside the main thread");
        std::process::abort();
    }
}

pub fn call(operation: impl FnOnce() -> Result<BridgeResult, Error>) -> BridgeResult {
    require_main_thread();
    let _runtime = TOKIO_HANDLE.get().map(tokio::runtime::Handle::enter);
    // Every ABI entry is extern "C": Rust aborts if a panic reaches that
    // non-unwinding boundary. Only ordinary errors cross as error results.
    match operation() {
        Ok(value) => value,
        Err(error) => BridgeResult::error(FAILED, error.to_string()),
    }
}

pub fn object<T>(value: T) -> BridgeResult {
    require_main_thread();
    BridgeResult {
        handle: Rc::into_raw(Rc::new(value)).cast(),
        ..BridgeResult::empty(OK)
    }
}

/// # Safety
/// `handle` must name a live Rc<T> created by `object`; it is borrowed, not consumed.
pub unsafe fn borrow_object<T>(handle: *const c_void) -> Rc<T> {
    require_main_thread();
    assert!(!handle.is_null(), "null object handle");
    let pointer = handle.cast::<T>();
    unsafe {
        Rc::increment_strong_count(pointer);
        Rc::from_raw(pointer)
    }
}

/// # Safety
/// Consume a live, correctly typed owned handle. The caller must stop using it.
pub unsafe fn own_object<T>(handle: *const c_void) -> Rc<T> {
    require_main_thread();
    assert!(!handle.is_null(), "null object handle");
    unsafe { Rc::from_raw(handle.cast::<T>()) }
}

/// # Safety
/// Consume exactly one owned reference previously returned by `object`.
pub unsafe fn free_object<T>(handle: *const c_void) -> BridgeResult {
    call(|| {
        assert!(!handle.is_null(), "null object handle");
        unsafe { drop(Rc::from_raw(handle.cast::<T>())) };
        Ok(().into_result())
    })
}

#[repr(C)]
pub struct HostCallback {
    pub context: usize,
    pub invoke: Option<extern "C" fn(usize, BridgeResult) -> u8>,
    pub release: Option<extern "C" fn(usize)>,
}

impl Drop for HostCallback {
    fn drop(&mut self) {
        require_main_thread();
        if let Some(release) = self.release {
            release(self.context);
        }
    }
}

impl<T: CallbackArguments + 'static> Callback<T> {
    /// # Safety
    /// `callback` transfers an owned context with matching invoke/release functions.
    pub unsafe fn from_host(callback: HostCallback) -> Self {
        require_main_thread();
        Self::new(move |value: T| {
            require_main_thread();
            // Borrow the whole Drop-bearing struct; don't capture just its Copy fields.
            invoke_callback(&callback, value.into_callback()?)?;
            Ok(())
        })
    }
}
fn invoke_callback(callback: &HostCallback, value: BridgeResult) -> Result<(), Error> {
    if (callback.invoke.expect("missing callback function"))(callback.context, value) == 0 {
        Err(Error::new("host callback could not decode its arguments"))
    } else {
        Ok(())
    }
}

type WakeCallback = extern "C" fn(u64);

/// Background threads can own only this notification. It has no pointer to the task/object.
#[derive(Default)]
struct Signal(Mutex<Option<(WakeCallback, u64)>>);

impl Wake for Signal {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        let callback = *self.0.lock().unwrap();
        // Host callback only enqueues an ID. A racing late wake is harmless after unregister.
        if let Some((callback, context)) = callback {
            callback(context);
        }
    }
}

type LocalFuture = Pin<Box<dyn Future<Output = Result<BridgeResult, Error>>>>;

struct NativeTask {
    future: std::cell::RefCell<Option<LocalFuture>>,
    signal: Arc<Signal>,
}

impl Drop for NativeTask {
    fn drop(&mut self) {
        require_main_thread();
        *self.signal.0.lock().unwrap() = None;
    }
}

pub fn task(future: impl Future<Output = Result<BridgeResult, Error>> + 'static) -> BridgeResult {
    object(NativeTask {
        future: std::cell::RefCell::new(Some(Box::pin(future))),
        signal: Arc::default(),
    })
}

/// # Safety
/// `handle` must be a live task. The callback can run on background threads, including after
/// task release when racing with unregister. It must remain callable for such late wakes and
/// only schedule owner-thread work. Generated bindings use a static function and non-reused ID.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bridgerton_task_poll(
    handle: *const c_void,
    callback: WakeCallback,
    context: u64,
) -> BridgeResult {
    call(|| {
        let task = unsafe { borrow_object::<NativeTask>(handle) };
        let mut future_slot = task
            .future
            .try_borrow_mut()
            .map_err(|_| Error::new("reentrant future poll"))?;
        let future = future_slot
            .as_mut()
            .ok_or_else(|| Error::new("future already completed"))?;
        *task.signal.0.lock().unwrap() = Some((callback, context));
        let waker = Waker::from(task.signal.clone());
        match future.as_mut().poll(&mut Context::from_waker(&waker)) {
            Poll::Pending => Ok(BridgeResult::empty(PENDING)),
            Poll::Ready(result) => {
                // Drop application state with no RefCell borrow held across destructors.
                let completed = future_slot.take();
                drop(future_slot);
                *task.signal.0.lock().unwrap() = None;
                drop(completed);
                result
            }
        }
    })
}

/// # Safety
/// Consume one task handle; no subsequent poll/free may use it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bridgerton_task_free(handle: *const c_void) -> BridgeResult {
    unsafe { free_object::<NativeTask>(handle) }
}

/// # Safety
/// Consume exactly once a buffer returned by this library, without modifying its fields.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bridgerton_buffer_free(buffer: Buffer) {
    if !buffer.data.is_null() {
        unsafe {
            drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                buffer.data,
                buffer.len,
            )))
        };
    }
}

pub struct Definition {
    pub header: String,
    pub swift: String,
    pub types: crate::schema::Registry,
}

// Rust symbols carry the full module/type path. C aliases use its lossless hex
// spelling, so equally named types in separate modules cannot collide at link time.
#[doc(hidden)]
pub fn qualify_export(header: &str, swift: &str, scope: &str, symbol: &str) -> (String, String) {
    use std::fmt::Write;
    let mut identifier = String::from("bridgerton_");
    for byte in scope.bytes() {
        write!(identifier, "{byte:02x}").unwrap();
    }
    identifier.push('_');
    identifier.push_str(symbol);
    let prefix = if cfg!(target_vendor = "apple") {
        "_"
    } else {
        ""
    };
    let header = header
        .replace(symbol, &identifier)
        .replace(";", &format!(" __asm__(\"{prefix}{scope}::{symbol}\");"));
    (header, swift.replace(symbol, &identifier))
}

// The handshake has a deliberately fixed ABI and runs before any generated call.
// Hash both declarations and the codec/runtime implementation: unchanged C symbols
// alone cannot detect a reordered record or a changed binary representation.
fn binding_sources() -> Result<(String, String), Error> {
    let definition = crate::exports::definition()?;
    definition.types.validate()?;
    Ok((
        format!("{}{}", include_str!("runtime.h"), definition.header),
        format!(
            "{}{}{}{}",
            include_str!("runtime.swift"),
            include_str!("value.swift"),
            definition.swift,
            definition.types.swift()
        ),
    ))
}

fn fingerprint(header: &str, swift: &str) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    for part in [
        header,
        swift,
        include_str!("native.rs"),
        include_str!("native/bindings.rs"),
        include_str!("native/returns.rs"),
        include_str!("native/callbacks.rs"),
        include_str!("abort.rs"),
        include_str!("value.rs"),
    ] {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part.as_bytes());
    }
    hash.finalize().into()
}

/// # Safety
/// `expected` must point to `len` readable bytes. This versioned handshake never
/// accepts object handles or returns an allocation whose layout could differ.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bridgerton_abi_v1_matches(expected: *const u8, len: usize) -> u8 {
    static ACTUAL: std::sync::OnceLock<[u8; 32]> = std::sync::OnceLock::new();
    if expected.is_null() || len != 32 {
        return 0;
    }
    let actual = ACTUAL.get_or_init(|| {
        let (header, swift) = binding_sources().expect("invalid bridge definitions");
        fingerprint(&header, &swift)
    });
    u8::from(unsafe { std::slice::from_raw_parts(expected, len) } == actual)
}

pub fn generate(output: &std::path::Path) -> std::io::Result<()> {
    let (header, swift) = binding_sources().map_err(std::io::Error::other)?;
    let expected = fingerprint(&header, &swift)
        .map(|byte| byte.to_string())
        .join(", ");
    let swift = swift.replace("@bridge_interface_bytes@", &expected);
    std::fs::create_dir_all(output)?;
    std::fs::write(output.join("BridgeFFI.h"), header)?;
    std::fs::write(output.join("Bridge.swift"), swift)?;
    std::fs::write(
        output.join("module.modulemap"),
        "module BridgeFFI { header \"BridgeFFI.h\" export * }\n",
    )
}

/// Build-tool entry point. The generator loads the host's cdylib and invokes
/// this function; no application object or event store is created.
/// # Safety
/// `path` must point to `len` readable UTF-8 bytes for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bridgerton_generate_v1(path: *const u8, len: usize) -> BridgeResult {
    call(|| {
        if path.is_null() {
            return Err(Error::new("null output path"));
        }
        let bytes = unsafe { std::slice::from_raw_parts(path, len) };
        let path = std::str::from_utf8(bytes).map_err(|e| Error::new(e.to_string()))?;
        generate(std::path::Path::new(path))?;
        Ok(().into_result())
    })
}

static TOKIO_HANDLE: std::sync::OnceLock<tokio::runtime::Handle> = std::sync::OnceLock::new();

/// Enter this host-owned runtime around native calls and future polls.
/// Install once on the main thread, before calling APIs that use Tokio.
/// The host must keep the Runtime alive and drive it for as long as those APIs
/// are in use; a Handle alone does not keep its drivers running.
/// The bridge never creates a runtime or moves confined futures onto it.
pub fn set_tokio_handle(handle: tokio::runtime::Handle) -> Result<(), Error> {
    require_main_thread();
    TOKIO_HANDLE
        .set(handle)
        .map_err(|_| Error::new("bridge Tokio handle is already installed"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waker_can_outlive_task_registration_and_be_dropped_on_another_thread() {
        let signal = Arc::new(Signal::default());
        let waker = Waker::from(signal.clone());
        std::thread::spawn(move || {
            waker.wake_by_ref();
            drop(waker);
        })
        .join()
        .unwrap();
        assert_eq!(Arc::strong_count(&signal), 1);
    }
}
