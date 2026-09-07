//! Callback arguments reuse the owned return transport, including opaque objects.
use super::*;
use crate::schema::{NativeType, Registry, Type};

pub trait CallbackArguments: Sized {
    fn callback_types(registry: &mut Registry) -> Vec<Type>;
    fn into_callback(self) -> Result<BridgeResult, Error>
    where
        Self: 'static;
}

pub type CallbackSlot = Box<dyn FnOnce() -> Result<BridgeResult, Error>>;

pub fn callback_arguments(slots: Vec<CallbackSlot>) -> Result<BridgeResult, Error> {
    returns::iterator(slots.into_iter().map(|slot| slot()))
}

impl CallbackArguments for () {
    fn callback_types(_: &mut Registry) -> Vec<Type> {
        vec![]
    }
    fn into_callback(self) -> Result<BridgeResult, Error> {
        callback_arguments(vec![])
    }
}
impl<
    A: NativeReturn<Success = A> + NativeType + 'static,
    B: NativeReturn<Success = B> + NativeType + 'static,
> CallbackArguments for (A, B)
{
    fn callback_types(r: &mut Registry) -> Vec<Type> {
        vec![A::native_type(r), B::native_type(r)]
    }
    fn into_callback(self) -> Result<BridgeResult, Error> {
        callback_arguments(vec![
            Box::new(move || self.0.into_native_return()),
            Box::new(move || self.1.into_native_return()),
        ])
    }
}
impl<
    A: NativeReturn<Success = A> + NativeType + 'static,
    B: NativeReturn<Success = B> + NativeType + 'static,
    C: NativeReturn<Success = C> + NativeType + 'static,
> CallbackArguments for (A, B, C)
{
    fn callback_types(r: &mut Registry) -> Vec<Type> {
        vec![A::native_type(r), B::native_type(r), C::native_type(r)]
    }
    fn into_callback(self) -> Result<BridgeResult, Error> {
        callback_arguments(vec![
            Box::new(move || self.0.into_native_return()),
            Box::new(move || self.1.into_native_return()),
            Box::new(move || self.2.into_native_return()),
        ])
    }
}
