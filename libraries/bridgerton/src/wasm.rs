//! Type-directed WASM conversion. Rust resolves aliases before selecting an ABI.
use std::marker::PhantomData;
use wasm_bindgen::{
    JsValue,
    convert::{FromWasmAbi, IntoWasmAbi, OptionFromWasmAbi},
    describe::{NAMED_EXTERNREF, WasmDescribe, inform},
};

pub trait WasmType {
    const INPUT_LEN: u32;
    const OUTPUT_LEN: u32 = Self::INPUT_LEN;
    const ELEMENT_LEN: u32 = Self::OUTPUT_LEN;
    fn describe_name<const INPUT: bool>();
    fn describe_element() {
        Self::describe_name::<false>();
    }
    const ARRAY_INPUT_LEN: u32 = 7 + Self::INPUT_LEN;
    const ARRAY_OUTPUT_LEN: u32 = 7 + Self::ELEMENT_LEN;
    fn describe_array<const INPUT: bool>() {
        crate::__describe!("Array<");
        if INPUT {
            Self::describe_name::<true>();
        } else {
            Self::describe_element();
        }
        crate::__describe!(">");
    }
}

/// A real JS reference with a type-directed TypeScript descriptor.
pub struct TypedJs<T, const INPUT: bool>(pub JsValue, PhantomData<T>);
impl<T, const INPUT: bool> From<JsValue> for TypedJs<T, INPUT> {
    fn from(value: JsValue) -> Self {
        Self(value, PhantomData)
    }
}
impl<T, const INPUT: bool> From<TypedJs<T, INPUT>> for JsValue {
    fn from(value: TypedJs<T, INPUT>) -> Self {
        value.0
    }
}
impl<T: WasmType, const INPUT: bool> WasmDescribe for TypedJs<T, INPUT> {
    fn describe() {
        inform(NAMED_EXTERNREF);
        inform(if INPUT { T::INPUT_LEN } else { T::OUTPUT_LEN });
        T::describe_name::<INPUT>();
    }
}
impl<T: WasmType, const INPUT: bool> IntoWasmAbi for TypedJs<T, INPUT> {
    type Abi = <JsValue as IntoWasmAbi>::Abi;
    fn into_abi(self) -> Self::Abi {
        self.0.into_abi()
    }
}
impl<T: WasmType, const INPUT: bool> FromWasmAbi for TypedJs<T, INPUT> {
    type Abi = <JsValue as FromWasmAbi>::Abi;
    unsafe fn from_abi(value: Self::Abi) -> Self {
        Self::from(unsafe { JsValue::from_abi(value) })
    }
}

impl<T: WasmType, const INPUT: bool> OptionFromWasmAbi for TypedJs<T, INPUT> {
    fn is_none(value: &Self::Abi) -> bool {
        <JsValue as OptionFromWasmAbi>::is_none(value)
    }
}

pub trait FromWasm: Sized {
    type Input: FromWasmAbi + WasmDescribe;
    fn from_wasm(value: Self::Input) -> Result<Self, JsValue>;
    fn from_js(value: JsValue) -> Result<Self, JsValue>;
}
pub trait IntoWasm: Sized {
    type Output: IntoWasmAbi + WasmDescribe + Into<JsValue>;
    fn into_wasm(self) -> Result<Self::Output, JsValue>;
    fn into_js(self) -> Result<JsValue, JsValue> {
        self.into_wasm().map(Into::into)
    }
    fn into_element(self) -> Result<JsValue, JsValue> {
        self.into_js()
    }
    fn array_to_js(values: Vec<Self>) -> Result<JsValue, JsValue> {
        let array = js_sys::Array::new();
        for value in values {
            array.push(&value.into_element()?);
        }
        Ok(array.into())
    }
}
macro_rules! scalar {
    ($ty:ty, $name:literal $(, $array:ident)?) => {
        impl WasmType for $ty {
            const INPUT_LEN: u32 = $name.len() as u32;
            fn describe_name<const INPUT: bool>() { crate::__describe!($name); }
            $(const ARRAY_INPUT_LEN: u32 = stringify!($array).len() as u32;
              const ARRAY_OUTPUT_LEN: u32 = Self::ARRAY_INPUT_LEN;
              fn describe_array<const INPUT: bool>() { crate::__describe!($array); })?
        }
        impl FromWasm for $ty {
            type Input = Self;
            fn from_wasm(value: Self) -> Result<Self, JsValue> { Ok(value) }
            fn from_js(value: JsValue) -> Result<Self, JsValue> { crate::from_js(value) }
        }
        impl IntoWasm for $ty {
            type Output = Self;
            fn into_wasm(self) -> Result<Self, JsValue> { Ok(self) }
            $(fn array_to_js(values: Vec<Self>) -> Result<JsValue, JsValue> {
                let array = js_sys::$array::new_with_length(values.len() as u32);
                for (index, value) in values.into_iter().enumerate() { array.set_index(index as u32, value as _); }
                Ok(array.into())
            })?
        }
    }
}
scalar!(u8, "number", Uint8Array);
scalar!(i8, "number", Int8Array);
scalar!(u16, "number", Uint16Array);
scalar!(i16, "number", Int16Array);
scalar!(u32, "number", Uint32Array);
scalar!(i32, "number", Int32Array);
scalar!(usize, "number", Uint32Array);
scalar!(isize, "number", Int32Array);
scalar!(u64, "bigint", BigUint64Array);
scalar!(i64, "bigint", BigInt64Array);
scalar!(f32, "number", Float32Array);
scalar!(f64, "number", Float64Array);
scalar!(bool, "boolean");
scalar!(String, "string");
impl WasmType for () {
    const INPUT_LEN: u32 = 4;
    fn describe_name<const INPUT: bool>() {
        crate::__describe!("void");
    }
}
impl IntoWasm for () {
    type Output = TypedJs<Self, false>;
    fn into_wasm(self) -> Result<Self::Output, JsValue> {
        Ok(JsValue::UNDEFINED.into())
    }
}

impl<T: WasmType> WasmType for Vec<T> {
    const INPUT_LEN: u32 = T::ARRAY_INPUT_LEN;
    const OUTPUT_LEN: u32 = T::ARRAY_OUTPUT_LEN;
    fn describe_name<const INPUT: bool>() {
        T::describe_array::<INPUT>();
    }
}
impl<T: FromWasm + WasmType> FromWasm for Vec<T> {
    type Input = TypedJs<Self, true>;
    fn from_wasm(value: Self::Input) -> Result<Self, JsValue> {
        Self::from_js(value.0)
    }
    fn from_js(value: JsValue) -> Result<Self, JsValue> {
        if !js_sys::Array::is_array(&value) && !js_sys::ArrayBuffer::is_view(&value) {
            return Err(JsValue::from_str("expected an array or typed array"));
        }
        js_sys::Array::from(&value).iter().map(T::from_js).collect()
    }
}
impl<T: IntoWasm + WasmType> IntoWasm for Vec<T> {
    type Output = TypedJs<Self, false>;
    fn into_wasm(self) -> Result<Self::Output, JsValue> {
        T::array_to_js(self).map(Into::into)
    }
}
impl<T: WasmType> WasmType for Option<T> {
    const INPUT_LEN: u32 = 1 + T::INPUT_LEN + " | null | undefined)".len() as u32;
    const OUTPUT_LEN: u32 = 1 + T::OUTPUT_LEN + " | undefined)".len() as u32;
    const ELEMENT_LEN: u32 = 1 + T::ELEMENT_LEN + " | null)".len() as u32;
    fn describe_name<const INPUT: bool>() {
        crate::__describe!("(");
        T::describe_name::<INPUT>();
        if INPUT {
            crate::__describe!(" | null | undefined)");
        } else {
            crate::__describe!(" | undefined)");
        }
    }
    fn describe_element() {
        crate::__describe!("(");
        T::describe_element();
        crate::__describe!(" | null)");
    }
}
impl<T: FromWasm + WasmType> FromWasm for Option<T> {
    type Input = Option<TypedJs<T, true>>;
    fn from_wasm(value: Self::Input) -> Result<Self, JsValue> {
        Self::from_js(value.map_or(JsValue::UNDEFINED, |value| value.0))
    }
    fn from_js(value: JsValue) -> Result<Self, JsValue> {
        if value.is_null() || value.is_undefined() {
            Ok(None)
        } else {
            T::from_js(value).map(Some)
        }
    }
}
impl<T: IntoWasm + WasmType> IntoWasm for Option<T> {
    type Output = TypedJs<Self, false>;
    fn into_wasm(self) -> Result<Self::Output, JsValue> {
        self.map_or(Ok(JsValue::UNDEFINED), T::into_js)
            .map(Into::into)
    }
    fn into_element(self) -> Result<JsValue, JsValue> {
        self.map_or(Ok(JsValue::NULL), T::into_element)
    }
}
/// Errors cross to JavaScript as `Error` objects. Every error carries its
/// message; typed errors also carry a `detail` property shaped like the value
/// Swift receives.
pub trait WasmError {
    fn into_js_error(self) -> JsValue;
}

#[doc(hidden)]
pub fn js_error(message: &str, detail: Option<JsValue>) -> JsValue {
    let error = js_sys::Error::new(message);
    if let Some(detail) = detail {
        let _ = js_sys::Reflect::set(&error, &JsValue::from_str("detail"), &detail);
    }
    error.into()
}

impl WasmError for crate::Error {
    fn into_js_error(self) -> JsValue {
        js_error(&self.to_string(), None)
    }
}
impl WasmError for String {
    fn into_js_error(self) -> JsValue {
        js_error(&self, None)
    }
}
impl WasmError for &str {
    fn into_js_error(self) -> JsValue {
        js_error(self, None)
    }
}
impl WasmError for abort_signal::Aborted {
    fn into_js_error(self) -> JsValue {
        js_error(&self.to_string(), None)
    }
}
/// A raw JavaScript error, for example from a browser API, is rethrown as is.
impl WasmError for JsValue {
    fn into_js_error(self) -> JsValue {
        self
    }
}
impl WasmError for std::io::Error {
    fn into_js_error(self) -> JsValue {
        let message = self.to_string();
        let detail = crate::io_error::IoError::from(self).into_js().ok();
        js_error(&message, detail)
    }
}
impl<T: IntoWasm, E: WasmError> IntoWasm for Result<T, E> {
    type Output = T::Output;
    fn into_wasm(self) -> Result<Self::Output, JsValue> {
        self.map_err(WasmError::into_js_error)?.into_wasm()
    }
}

impl WasmType for JsValue {
    const INPUT_LEN: u32 = 3;
    fn describe_name<const INPUT: bool>() {
        crate::__describe!("any");
    }
}
impl FromWasm for JsValue {
    type Input = Self;
    fn from_wasm(value: Self) -> Result<Self, JsValue> {
        Ok(value)
    }
    fn from_js(value: Self) -> Result<Self, JsValue> {
        Ok(value)
    }
}
impl IntoWasm for JsValue {
    type Output = Self;
    fn into_wasm(self) -> Result<Self, JsValue> {
        Ok(self)
    }
}

#[doc(hidden)]
#[macro_export]
macro_rules! __wasm_object {
    ($ty:ident, $name:literal, $name_len:literal) => {
        impl $crate::WasmType for $ty {
            const INPUT_LEN: u32 = $name_len;
            fn describe_name<const INPUT: bool>() {
                $crate::__describe!($name);
            }
        }
        impl $crate::FromWasm for $ty {
            type Input = Self;
            fn from_wasm(value: Self) -> Result<Self, $crate::__wasm_bindgen::JsValue> {
                Ok(value)
            }
            fn from_js(
                value: $crate::__wasm_bindgen::JsValue,
            ) -> Result<Self, $crate::__wasm_bindgen::JsValue> {
                <Self as $crate::__wasm_bindgen::convert::TryFromJsValue>::try_from_js_value(value)
            }
        }
        impl $crate::IntoWasm for $ty {
            type Output = Self;
            fn into_wasm(self) -> Result<Self, $crate::__wasm_bindgen::JsValue> {
                Ok(self)
            }
        }
    };
}

impl<T: crate::JsArguments + 'static> FromWasm for crate::Callback<T> {
    type Input = Self;
    fn from_wasm(value: Self) -> Result<Self, JsValue> {
        Ok(value)
    }
    fn from_js(value: JsValue) -> Result<Self, JsValue> {
        use wasm_bindgen::JsCast;
        value.dyn_into::<js_sys::Function>().map(Self::from_js)
    }
}
impl<T> WasmType for crate::Callback<T> {
    const INPUT_LEN: u32 = 8;
    fn describe_name<const INPUT: bool>() {
        crate::__describe!("Function");
    }
}
impl FromWasm for crate::AbortSignal {
    type Input = Self;
    fn from_wasm(value: Self) -> Result<Self, JsValue> {
        Ok(value)
    }
    fn from_js(value: JsValue) -> Result<Self, JsValue> {
        use wasm_bindgen::JsCast;
        value.dyn_into::<web_sys::AbortSignal>().map(Into::into)
    }
}
impl WasmType for crate::AbortSignal {
    const INPUT_LEN: u32 = 11;
    fn describe_name<const INPUT: bool>() {
        crate::__describe!("AbortSignal");
    }
}
impl IntoWasm for js_sys::Uint8Array {
    type Output = Self;
    fn into_wasm(self) -> Result<Self, JsValue> {
        Ok(self)
    }
}

/// The representation inside a Serde value differs from the direct function ABI:
/// e.g. Vec<u32> is an ordinary JS array, and u64 is a number unless configured.
#[doc(hidden)]
pub trait SerdeType<const BIGINT: bool, const NULL: bool, const OBJECTS: bool> {
    const LEN: u32;
    fn describe();
}
macro_rules! serde_scalar {
    ($($ty:ty),*) => {$(
        impl<const B: bool, const N: bool, const O: bool> SerdeType<B, N, O> for $ty {
            const LEN: u32 = <$ty as WasmType>::INPUT_LEN;
            fn describe() { <$ty as WasmType>::describe_name::<true>(); }
        }
    )*};
}
serde_scalar!(
    u8, i8, u16, i16, u32, i32, usize, isize, f32, f64, bool, String
);
macro_rules! serde_integer {
    ($($ty:ty),*) => {$(
        impl<const B: bool, const N: bool, const O: bool> SerdeType<B, N, O> for $ty {
            const LEN: u32 = 6;
            fn describe() { if B { crate::__describe!("bigint"); } else { crate::__describe!("number"); } }
        }
    )*};
}
serde_integer!(u64, i64, u128, i128);
impl<T: SerdeType<B, N, O>, const B: bool, const N: bool, const O: bool> SerdeType<B, N, O>
    for Vec<T>
{
    const LEN: u32 = 7 + T::LEN;
    fn describe() {
        crate::__describe!("Array<");
        T::describe();
        crate::__describe!(">");
    }
}
impl<T: SerdeType<B, N, O>, const B: bool, const N: bool, const O: bool> SerdeType<B, N, O>
    for Option<T>
{
    const LEN: u32 = T::LEN + if N { 9 } else { 14 };
    fn describe() {
        crate::__describe!("(");
        T::describe();
        if N {
            crate::__describe!(" | null)");
        } else {
            crate::__describe!(" | undefined)");
        }
    }
}
impl<T: SerdeType<B, N, O>, const B: bool, const N: bool, const O: bool> SerdeType<B, N, O>
    for Box<T>
{
    const LEN: u32 = T::LEN;
    fn describe() {
        T::describe();
    }
}
impl<A: SerdeType<B, N, O>, Z: SerdeType<B, N, O>, const B: bool, const N: bool, const O: bool>
    SerdeType<B, N, O> for (A, Z)
{
    const LEN: u32 = A::LEN + Z::LEN + 4;
    fn describe() {
        crate::__describe!("[");
        A::describe();
        crate::__describe!(", ");
        Z::describe();
        crate::__describe!("]");
    }
}
