//! Type-directed method transport. Rust resolves aliases before choosing these impls.
use super::*;
use crate::{
    schema::{NativeType, Registry, Type},
    value::Value,
};

pub trait NativeError: Sized {
    fn into_error_result(self) -> Result<BridgeResult, Error>;
    /// None means the existing message-only BridgeError transport.
    fn error_type(registry: &mut Registry) -> Option<String>;
}
impl<T: Value + NativeType> NativeError for T {
    fn into_error_result(self) -> Result<BridgeResult, Error> {
        let mut result = value(&self)?;
        result.status = TYPED_ERROR;
        Ok(result)
    }
    fn error_type(registry: &mut Registry) -> Option<String> {
        Some(registry.error::<T>())
    }
}
impl NativeError for Error {
    fn into_error_result(self) -> Result<BridgeResult, Error> {
        Err(self)
    }
    fn error_type(_: &mut Registry) -> Option<String> {
        None
    }
}

pub trait NativeReturn: Sized {
    type Success: NativeType;
    const FALLIBLE: bool = false;
    fn into_native_return(self) -> Result<BridgeResult, Error>;
    fn return_array(values: Vec<Self>) -> Result<BridgeResult, Error>
    where
        Self: 'static,
    {
        super::returns::sequence(values)
    }
    fn return_optional(value: Option<Self>) -> Result<BridgeResult, Error>
    where
        Self: 'static,
    {
        super::returns::sequence(value.into_iter().collect())
    }
    fn error_type(_: &mut Registry) -> Option<String> {
        None
    }
}
impl<T: NativeReturn, E: NativeError> NativeReturn for Result<T, E> {
    type Success = T::Success;
    const FALLIBLE: bool = true;
    fn into_native_return(self) -> Result<BridgeResult, Error> {
        match self {
            Ok(value) => value.into_native_return(),
            Err(error) => error.into_error_result(),
        }
    }
    fn error_type(registry: &mut Registry) -> Option<String> {
        assert!(!T::FALLIBLE, "nested Result returns are unsupported");
        E::error_type(registry)
    }
}

#[diagnostic::on_unimplemented(
    message = "borrowed native arguments must be bridged objects; pass ordinary values by ownership"
)]
pub trait NativeObject: NativeType {}

/// The wrapper owns this storage for the entire call, including async suspension.
pub trait NativeBorrowed {
    type Owned: NativeArgument;
    fn borrow(value: &Self::Owned) -> &Self;
}
impl NativeBorrowed for str {
    type Owned = String;
    fn borrow(value: &String) -> &str {
        value
    }
}
impl<T> NativeBorrowed for [T]
where
    Vec<T>: NativeArgument,
{
    type Owned = Vec<T>;
    fn borrow(value: &Vec<T>) -> &[T] {
        value
    }
}
pub struct BorrowedObject<T>(Rc<T>);
impl<T: NativeObject> NativeBorrowed for T {
    type Owned = BorrowedObject<T>;
    fn borrow(value: &BorrowedObject<T>) -> &T {
        &value.0
    }
}
impl<T: NativeObject> NativeArgument for BorrowedObject<T> {
    type Abi = *const c_void;
    type Prepared = Self;
    const C_TYPE: &'static str = "const void *";
    unsafe fn prepare_native(value: Self::Abi) -> Self {
        Self(unsafe { borrow_object(value) })
    }
    unsafe fn from_prepared(value: Self) -> Result<Self, Error> {
        Ok(value)
    }
    fn swift_type(registry: &mut Registry) -> String {
        T::native_type(registry).swift()
    }
    fn argument(name: &str) -> String {
        format!("{name}.__bridgertonHandle")
    }
    fn wrap(name: &str, call: String) -> String {
        format!("withExtendedLifetime({name}) {{ {call} }}")
    }
}

pub trait NativeArgument: Sized {
    type Abi;
    type Prepared;
    const C_TYPE: &'static str;
    /// # Safety
    /// The ABI value must satisfy the generated caller's ownership and pointer contracts.
    unsafe fn prepare_native(value: Self::Abi) -> Self::Prepared;
    /// # Safety
    /// Prepared borrowed buffers must remain valid until decoding finishes.
    unsafe fn from_prepared(value: Self::Prepared) -> Result<Self, Error>;
    /// Local child signal wiring for asynchronous calls, if this argument supports cancellation.
    fn task_signal(_: &str) -> Option<String> {
        None
    }
    fn swift_type(registry: &mut Registry) -> String;
    fn argument(name: &str) -> String;
    fn wrap(name: &str, call: String) -> String;
}
pub trait NativeOptionalArgument: Sized {
    const C_TYPE: &'static str;
    type Abi;
    type Prepared;
    /// # Safety
    /// The input must satisfy this type's ABI and ownership contract.
    unsafe fn prepare_optional(value: Self::Abi) -> Self::Prepared;
    /// # Safety
    /// Borrowed buffers in the prepared value must remain valid for decoding.
    unsafe fn from_optional(value: Self::Prepared) -> Result<Option<Self>, Error>;
    fn optional_type(registry: &mut Registry) -> String;
    fn optional_argument(name: &str) -> String;
    fn optional_wrap(name: &str, call: String) -> String;
    fn optional_task_signal(_: &str) -> Option<String> {
        None
    }
}
impl<T: NativeOptionalArgument> NativeArgument for Option<T> {
    type Abi = T::Abi;
    type Prepared = T::Prepared;
    const C_TYPE: &'static str = T::C_TYPE;
    unsafe fn prepare_native(value: Self::Abi) -> Self::Prepared {
        unsafe { T::prepare_optional(value) }
    }
    unsafe fn from_prepared(value: Self::Prepared) -> Result<Self, Error> {
        unsafe { T::from_optional(value) }
    }
    fn swift_type(r: &mut Registry) -> String {
        T::optional_type(r)
    }
    fn argument(name: &str) -> String {
        T::optional_argument(name)
    }
    fn wrap(name: &str, call: String) -> String {
        T::optional_wrap(name, call)
    }
    fn task_signal(name: &str) -> Option<String> {
        T::optional_task_signal(name)
    }
}
fn callback_type<T: super::CallbackArguments>(registry: &mut Registry) -> (String, usize) {
    let types = T::callback_types(registry);
    (
        types.iter().map(Type::swift).collect::<Vec<_>>().join(", "),
        types.len(),
    )
}
impl<T: super::CallbackArguments + 'static> NativeArgument for Callback<T> {
    type Abi = HostCallback;
    type Prepared = HostCallback;
    const C_TYPE: &'static str = "BridgeHostCallback";
    unsafe fn prepare_native(value: Self::Abi) -> Self::Prepared {
        value
    }
    unsafe fn from_prepared(value: Self::Prepared) -> Result<Self, Error> {
        if value.invoke.is_none() || value.release.is_none() {
            return Err(Error::new("missing callback"));
        }
        Ok(unsafe { Callback::from_host(value) })
    }
    fn swift_type(registry: &mut Registry) -> String {
        format!(
            "@escaping @MainActor ({}) -> Void",
            callback_type::<T>(registry).0
        )
    }
    fn argument(name: &str) -> String {
        format!(
            "bridgeCallback{}({name})",
            callback_type::<T>(&mut Registry::default()).1
        )
    }
    fn wrap(_: &str, call: String) -> String {
        call
    }
}
impl<T: super::CallbackArguments + 'static> NativeOptionalArgument for Callback<T> {
    type Abi = HostCallback;
    type Prepared = HostCallback;
    const C_TYPE: &'static str = "BridgeHostCallback";
    unsafe fn prepare_optional(value: Self::Abi) -> Self::Prepared {
        value
    }
    unsafe fn from_optional(value: Self::Prepared) -> Result<Option<Self>, Error> {
        if value.invoke.is_none() && value.release.is_none() && value.context == 0 {
            return Ok(None);
        }
        unsafe { Callback::<T>::from_prepared(value) }.map(Some)
    }
    fn optional_type(registry: &mut Registry) -> String {
        format!("(@MainActor ({}) -> Void)?", callback_type::<T>(registry).0)
    }
    fn optional_argument(name: &str) -> String {
        format!(
            "{name}.map {{ bridgeCallback{}($0) }} ?? BridgeHostCallback(context: 0, invoke: nil, release: nil)",
            callback_type::<T>(&mut Registry::default()).1
        )
    }
    fn optional_wrap(_: &str, call: String) -> String {
        call
    }
}

/// Render the return-dependent portion after Rust has resolved the full signature.
#[allow(clippy::too_many_arguments)]
pub fn return_method<R: NativeReturn>(
    registry: &mut Registry,
    name: &str,
    args: &str,
    invocation: &str,
    is_async: bool,
    is_static: bool,
    getter: bool,
    constructor: bool,
    class: &str,
    task_signals: &[(&str, String)],
) -> String {
    let ty = R::Success::native_type(registry);
    let error = R::error_type(registry);
    let unit = ty == Type::Scalar("BridgeUnit");
    let decoder = R::Success::return_decoder();
    let swift_type = if unit { "Void".into() } else { ty.swift() };
    let throws = if R::FALLIBLE { " throws" } else { "" };
    let attempt = if R::FALLIBLE { "try" } else { "try!" };
    let invocation = invocation.strip_prefix("try ").unwrap_or(invocation);
    let check = |call: &str| match &error {
        Some(error) => format!("bridgeTypedResult({call}, as: {error}.self)"),
        None => call.into(),
    };
    if constructor {
        assert_eq!(
            ty,
            Type::Named(class.into()),
            "constructor must return its object type"
        );
        return format!(
            "    public convenience init({args}){throws} {{\n        _ = BridgeInterface.checked\n        self.init(bridgeHandle: {attempt} bridgeHandle({}))\n    }}\n",
            check(invocation)
        );
    }
    let mut out = if getter {
        format!("    public var `{name}`: {swift_type} {{ get{throws} {{\n")
    } else {
        format!(
            "    public {}func `{name}`({args}){}{throws} -> {swift_type} {{\n",
            if is_static { "static " } else { "" },
            if is_async { " async" } else { "" }
        )
    };
    out += "        _ = BridgeInterface.checked\n";
    let result = if is_async {
        for (_, setup) in task_signals {
            out += setup;
        }
        out += &format!("        let task = {attempt} bridgeHandle({invocation})\n");
        format!(
            "await bridgeAwait(task, cancellable: {}, abortControllers: [{}])",
            R::FALLIBLE,
            task_signals
                .iter()
                .map(|(name, _)| format!("__abort_{name}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        invocation.into()
    };
    let result = check(&result);
    if unit {
        out += &format!("        let _: BridgeUnit = {attempt} bridgeReturn({result})\n");
    } else {
        out += &format!("        return {attempt} {decoder}({result})\n");
    }
    out += "    }\n";
    if getter {
        out += "    }\n";
    }
    out
}

impl NativeError for std::io::Error {
    fn into_error_result(self) -> Result<BridgeResult, Error> {
        crate::io_error::IoError::from(self).into_error_result()
    }
    fn error_type(registry: &mut Registry) -> Option<String> {
        <crate::io_error::IoError as NativeError>::error_type(registry)
    }
}

impl<T: NativeReturn<Success = T> + NativeType + 'static> NativeReturn for Vec<T> {
    type Success = Self;
    fn into_native_return(self) -> Result<BridgeResult, Error> {
        T::return_array(self)
    }
}
impl<T: NativeReturn<Success = T> + NativeType + 'static> NativeReturn for Option<T> {
    type Success = Self;
    fn into_native_return(self) -> Result<BridgeResult, Error> {
        T::return_optional(self)
    }
}
impl<T: NativeReturn<Success = T> + NativeType + 'static> NativeReturn for Box<T> {
    type Success = Self;
    fn into_native_return(self) -> Result<BridgeResult, Error> {
        (*self).into_native_return()
    }
    fn return_array(values: Vec<Self>) -> Result<BridgeResult, Error> {
        T::return_array(values.into_iter().map(|v| *v).collect())
    }
    fn return_optional(value: Option<Self>) -> Result<BridgeResult, Error> {
        T::return_optional(value.map(|v| *v))
    }
}
