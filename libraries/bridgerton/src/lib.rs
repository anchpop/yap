//! A deliberately small, experimental JavaScript/Swift bridge.
pub use bridgerton_macros::bridge;
#[doc(hidden)]
pub use bridgerton_macros::{
    __NativeValue, __TypeScript, __describe, __native_bridge, __native_function, __native_methods,
    __native_object, __wasm_bridge,
};
#[doc(hidden)]
pub use serde;
#[doc(hidden)]
pub use serde_json as __serde_json;
pub mod io_error;
pub mod platform;
pub mod value;

#[cfg(target_arch = "wasm32")]
pub fn from_js<T: serde::de::DeserializeOwned>(
    value: wasm_bindgen::JsValue,
) -> Result<T, wasm_bindgen::JsValue> {
    serde_wasm_bindgen::from_value(value)
        .map_err(|error| wasm_bindgen::JsValue::from_str(&error.to_string()))
}

/// Serialize a transparent value with its declared JavaScript representation.
#[doc(hidden)]
#[cfg(target_arch = "wasm32")]
pub fn to_js_with<T: serde::Serialize>(
    value: &T,
    large_number_types_as_bigints: bool,
    missing_as_null: bool,
    hashmap_as_object: bool,
) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
    value
        .serialize(
            &serde_wasm_bindgen::Serializer::new()
                .serialize_missing_as_null(missing_as_null)
                .serialize_maps_as_objects(hashmap_as_object)
                .serialize_large_number_types_as_bigints(large_number_types_as_bigints),
        )
        .map_err(|error| wasm_bindgen::JsValue::from_str(&error.to_string()))
}

// Generated wasm-bindgen code names its crates by their plain names, and its
// struct and custom-section paths ignore the crate-path override. The macro
// glob-imports this module beside every expansion so those names resolve
// without application crates declaring them. Repeated glob imports of the
// same items never conflict.
#[doc(hidden)]
#[cfg(target_arch = "wasm32")]
pub mod __wasm {
    pub use ::js_sys;
    pub use ::wasm_bindgen;
    // The struct expansion re-applies a bare `#[wasm_bindgen]` attribute.
    pub use ::wasm_bindgen::prelude::wasm_bindgen;
    pub use ::wasm_bindgen_futures;
}
#[doc(hidden)]
#[cfg(target_arch = "wasm32")]
pub use wasm_bindgen as __wasm_bindgen;

#[cfg(target_arch = "wasm32")]
mod wasm;
#[doc(hidden)]
#[cfg(target_arch = "wasm32")]
pub use wasm::{
    FromWasm, IntoWasm, SerdeType as __SerdeType, TypedJs, WasmError, WasmType, js_error,
};

use std::{fmt, rc::Rc};

// Allows macro expansion inside this crate to use the same absolute path as clients.
extern crate self as bridgerton;

#[cfg(not(target_arch = "wasm32"))]
pub mod exports;
#[cfg(not(target_arch = "wasm32"))]
pub mod native;
#[cfg(not(target_arch = "wasm32"))]
pub mod schema;
#[doc(hidden)]
#[cfg(not(target_arch = "wasm32"))]
pub use inventory as __inventory;

#[derive(Debug, Clone)]
pub struct Error(String);

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::new(error.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::new(error.to_string())
    }
}

#[cfg(target_arch = "wasm32")]
impl From<wasm_bindgen::JsValue> for Error {
    fn from(error: wasm_bindgen::JsValue) -> Self {
        Self::new(error.as_string().unwrap_or_else(|| format!("{error:?}")))
    }
}

/// An owned host callback. Cloning keeps the host closure alive. It is !Send/!Sync.
#[derive(Clone)]
pub struct Callback<T>(Rc<dyn Fn(T) -> Result<(), Error>>);

impl<T> Callback<T> {
    pub fn new(callback: impl Fn(T) -> Result<(), Error> + 'static) -> Self {
        Self(Rc::new(callback))
    }

    pub fn call(&self, value: T) -> Result<(), Error> {
        (self.0)(value)
    }
}

#[cfg(target_arch = "wasm32")]
pub trait JsArguments {
    fn invoke(
        self,
        function: &js_sys::Function,
    ) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue>;
}
#[cfg(target_arch = "wasm32")]
impl JsArguments for () {
    fn invoke(self, f: &js_sys::Function) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
        f.call0(&wasm_bindgen::JsValue::UNDEFINED)
    }
}
// One value argument: scalars and the generic wrappers use their ordinary
// return conversion, matching the native callback transport.
#[cfg(target_arch = "wasm32")]
macro_rules! single_js_argument {
    ($($ty:ty),*) => {$(
        impl JsArguments for $ty {
            fn invoke(self, f: &js_sys::Function) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
                f.call1(&wasm_bindgen::JsValue::UNDEFINED, &self.into_js()?)
            }
        }
    )*};
}
#[cfg(target_arch = "wasm32")]
single_js_argument!(
    u8, i8, u16, i16, u32, i32, u64, i64, usize, isize, f32, f64, bool, String
);
#[cfg(target_arch = "wasm32")]
impl<T: IntoWasm + WasmType> JsArguments for Option<T> {
    fn invoke(self, f: &js_sys::Function) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
        f.call1(&wasm_bindgen::JsValue::UNDEFINED, &self.into_js()?)
    }
}
#[cfg(target_arch = "wasm32")]
impl<T: IntoWasm + WasmType> JsArguments for Vec<T> {
    fn invoke(self, f: &js_sys::Function) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
        f.call1(&wasm_bindgen::JsValue::UNDEFINED, &self.into_js()?)
    }
}
#[cfg(target_arch = "wasm32")]
impl<A: IntoWasm, B: IntoWasm> JsArguments for (A, B)
where
    A::Output: Into<wasm_bindgen::JsValue>,
    B::Output: Into<wasm_bindgen::JsValue>,
{
    fn invoke(self, f: &js_sys::Function) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
        f.call2(
            &wasm_bindgen::JsValue::UNDEFINED,
            &self.0.into_wasm()?.into(),
            &self.1.into_wasm()?.into(),
        )
    }
}
#[cfg(target_arch = "wasm32")]
impl<T: JsArguments + 'static> Callback<T> {
    pub fn from_js(function: js_sys::Function) -> Self {
        Self::new(move |value: T| {
            value
                .invoke(&function)
                .map(|_| ())
                .map_err(|_| Error::new("host callback threw"))
        })
    }
}
#[cfg(target_arch = "wasm32")]
impl<A: IntoWasm, B: IntoWasm, C: IntoWasm> JsArguments for (A, B, C)
where
    A::Output: Into<wasm_bindgen::JsValue>,
    B::Output: Into<wasm_bindgen::JsValue>,
    C::Output: Into<wasm_bindgen::JsValue>,
{
    fn invoke(self, f: &js_sys::Function) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
        f.call3(
            &wasm_bindgen::JsValue::UNDEFINED,
            &self.0.into_wasm()?.into(),
            &self.1.into_wasm()?.into(),
            &self.2.into_wasm()?.into(),
        )
    }
}
// Support existing wasm-bindgen exports as well as our generated wrappers.
#[cfg(target_arch = "wasm32")]
impl<T: JsArguments + 'static> wasm_bindgen::describe::WasmDescribe for Callback<T> {
    fn describe() {
        <js_sys::Function as wasm_bindgen::describe::WasmDescribe>::describe();
    }
}
#[cfg(target_arch = "wasm32")]
impl<T: JsArguments + 'static> wasm_bindgen::convert::FromWasmAbi for Callback<T> {
    type Abi = <js_sys::Function as wasm_bindgen::convert::FromWasmAbi>::Abi;
    unsafe fn from_abi(value: Self::Abi) -> Self {
        Self::from_js(unsafe {
            <js_sys::Function as wasm_bindgen::convert::FromWasmAbi>::from_abi(value)
        })
    }
}
#[cfg(target_arch = "wasm32")]
impl<T: JsArguments + 'static> wasm_bindgen::convert::OptionFromWasmAbi for Callback<T> {
    fn is_none(value: &Self::Abi) -> bool {
        <js_sys::Function as wasm_bindgen::convert::OptionFromWasmAbi>::is_none(value)
    }
}
#[cfg(target_arch = "wasm32")]
impl From<Error> for wasm_bindgen::JsValue {
    fn from(error: Error) -> Self {
        Self::from_str(&error.to_string())
    }
}

mod abort;
pub use abort::{AbortController, AbortSignal};

/// Diagnostic text of an opaque error source for `#[bridge(message)]` fields:
/// any `Display` value, or on the web a raw JavaScript error value.
#[doc(hidden)]
pub mod message {
    pub trait ViaDisplay {
        fn __bridge_message(&self) -> String;
    }
    impl<T: std::fmt::Display> ViaDisplay for T {
        fn __bridge_message(&self) -> String {
            self.to_string()
        }
    }
    #[cfg(target_arch = "wasm32")]
    pub trait ViaJs {
        fn __bridge_message(&self) -> String;
    }
    #[cfg(target_arch = "wasm32")]
    impl ViaJs for &wasm_bindgen::JsValue {
        fn __bridge_message(&self) -> String {
            self.as_string().unwrap_or_else(|| format!("{self:?}"))
        }
    }
    /// Autoref specialization: a `Display` value resolves through `ViaDisplay`,
    /// a `JsValue` (which has no `Display`) through `ViaJs`.
    #[macro_export]
    #[doc(hidden)]
    macro_rules! __message_of {
        ($value:expr) => {{
            #[allow(unused_imports)]
            use $crate::message::*;
            (&$value).__bridge_message()
        }};
    }
}

impl From<abort_signal::Aborted> for Error {
    fn from(error: abort_signal::Aborted) -> Self {
        Self::new(error.to_string())
    }
}

// Ordinary values keep the bounded binary transport, including arrays/options.
#[doc(hidden)]
#[macro_export]
macro_rules! __native_value_return {
    () => {
        type Success = Self;
        fn into_native_return(self) -> Result<$crate::native::BridgeResult, $crate::Error> {
            <Self as $crate::schema::NativeType>::return_value(self)
        }
        fn return_array(values: Vec<Self>) -> Result<$crate::native::BridgeResult, $crate::Error> {
            $crate::native::value(&values)
        }
        fn return_optional(
            value: Option<Self>,
        ) -> Result<$crate::native::BridgeResult, $crate::Error> {
            $crate::native::value(&value)
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __native_callback_value {
    () => {
        fn callback_types(registry: &mut $crate::schema::Registry) -> Vec<$crate::schema::Type> {
            vec![<Self as $crate::schema::NativeType>::native_type(registry)]
        }
        fn into_callback(self) -> Result<$crate::native::BridgeResult, $crate::Error>
        where
            Self: 'static,
        {
            $crate::native::callback_arguments(vec![Box::new(move || {
                $crate::native::NativeReturn::into_native_return(self)
            })])
        }
    };
}

// The caller transfers ownership. Preparing every argument before decoding any
// of them guarantees cleanup if a later value is malformed or a move fails.
#[doc(hidden)]
#[macro_export]
macro_rules! __native_object_arguments {
    ($ty:ty) => {
        impl $crate::native::NativeArgument for $ty {
            type Abi = *const ::std::ffi::c_void;
            type Prepared = ::std::rc::Rc<Self>;
            const C_TYPE: &'static str = "const void *";
            unsafe fn prepare_native(handle: Self::Abi) -> Self::Prepared {
                unsafe { $crate::native::own_object(handle) }
            }
            unsafe fn from_prepared(value: Self::Prepared) -> Result<Self, $crate::Error> {
                ::std::rc::Rc::try_unwrap(value).map_err(|_| {
                    $crate::Error::new("cannot consume an object while Rust is borrowing it")
                })
            }
            fn swift_type(registry: &mut $crate::schema::Registry) -> String {
                <Self as $crate::schema::NativeType>::native_type(registry).swift()
            }
            fn argument(name: &str) -> String {
                format!("{name}.__bridgertonTakeHandle()")
            }
            fn wrap(_: &str, call: String) -> String {
                call
            }
        }
        impl $crate::native::NativeOptionalArgument for $ty {
            type Abi = *const ::std::ffi::c_void;
            type Prepared = Option<::std::rc::Rc<$ty>>;
            const C_TYPE: &'static str = "const void *";
            unsafe fn prepare_optional(handle: Self::Abi) -> Self::Prepared {
                if handle.is_null() {
                    None
                } else {
                    Some(unsafe { $crate::native::own_object(handle) })
                }
            }
            unsafe fn from_optional(value: Self::Prepared) -> Result<Option<Self>, $crate::Error> {
                value
                    .map(|value| unsafe {
                        <$ty as $crate::native::NativeArgument>::from_prepared(value)
                    })
                    .transpose()
            }
            fn optional_type(registry: &mut $crate::schema::Registry) -> String {
                format!(
                    "{}?",
                    <$ty as $crate::schema::NativeType>::native_type(registry).swift()
                )
            }
            fn optional_argument(name: &str) -> String {
                format!("{name}?.__bridgertonTakeHandle()")
            }
            fn optional_wrap(_: &str, call: String) -> String {
                call
            }
        }
    };
}

#[doc(hidden)]
#[cfg(target_arch = "wasm32")]
pub use js_sys as __js_sys;

#[doc(hidden)]
#[macro_export]
macro_rules! __native_value_argument {
    () => {
        type Abi = $crate::native::Bytes;
        type Prepared = $crate::native::Bytes;
        const C_TYPE: &'static str = "BridgeBytes";
        unsafe fn prepare_native(value: Self::Abi) -> Self::Prepared {
            value
        }
        unsafe fn from_prepared(value: Self::Prepared) -> Result<Self, $crate::Error> {
            unsafe { $crate::native::read_value(value) }
        }
        fn swift_type(registry: &mut $crate::schema::Registry) -> String {
            <Self as $crate::schema::NativeType>::native_type(registry).swift()
        }
        fn argument(name: &str) -> String {
            format!("__bytes_{name}")
        }
        fn wrap(name: &str, call: String) -> String {
            format!("try withBridgeValue({name}) {{ __bytes_{name} in {call} }}")
        }
    };
}
#[doc(hidden)]
#[macro_export]
macro_rules! __native_optional_value_argument {
    () => {
        type Abi = $crate::native::Bytes;
        type Prepared = $crate::native::Bytes;
        const C_TYPE: &'static str = "BridgeBytes";
        unsafe fn prepare_optional(value: Self::Abi) -> Self::Prepared {
            value
        }
        unsafe fn from_optional(value: Self::Prepared) -> Result<Option<Self>, $crate::Error> {
            unsafe { $crate::native::read_value(value) }
        }
        fn optional_type(registry: &mut $crate::schema::Registry) -> String {
            <Option<Self> as $crate::schema::NativeType>::native_type(registry).swift()
        }
        fn optional_argument(name: &str) -> String {
            format!("__bytes_{name}")
        }
        fn optional_wrap(name: &str, call: String) -> String {
            format!("try withBridgeValue({name}) {{ __bytes_{name} in {call} }}")
        }
    };
}

// A higher-ranked bound defers the conversion requirement until use, including
// for concrete, one-direction-only values. This also supports manual Serde impls;
// the macro never guesses capabilities by inspecting derive names.
#[doc(hidden)]
pub trait __Serializable<'a>: serde::Serialize {}
impl<T: serde::Serialize> __Serializable<'_> for T {}
