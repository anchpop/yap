//! Return-only containers of objects. The iterator owns every unclaimed element.
//! Converting one element at a time avoids putting owned handles in a byte buffer:
//! if Swift stops decoding, freeing the iterator drops all remaining Rust values.
use super::*;
use std::cell::RefCell;
struct ReturnSequence(RefCell<Box<dyn Iterator<Item = Result<BridgeResult, Error>>>>);

pub(super) fn sequence<T: NativeReturn + 'static>(values: Vec<T>) -> Result<BridgeResult, Error> {
    iterator(values.into_iter().map(NativeReturn::into_native_return))
}

pub(super) fn iterator(
    values: impl ExactSizeIterator<Item = Result<BridgeResult, Error>> + 'static,
) -> Result<BridgeResult, Error> {
    if values.len() > crate::value::MAX_ITEMS {
        return Err(Error::new("value exceeds item limit"));
    }
    let count = values.len() as u32;
    let mut result = object(ReturnSequence(RefCell::new(Box::new(values))));
    result.value = count;
    Ok(result)
}
/// # Safety
/// The handle must be a live, correctly typed sequence from generated bindings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bridgerton_sequence_next(handle: *const c_void) -> BridgeResult {
    call(|| {
        let sequence = unsafe { borrow_object::<ReturnSequence>(handle) };
        sequence
            .0
            .borrow_mut()
            .next()
            .ok_or_else(|| Error::new("return sequence exhausted"))?
    })
}
/// # Safety
/// Consume exactly one owned return-sequence handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bridgerton_sequence_free(handle: *const c_void) -> BridgeResult {
    unsafe { free_object::<ReturnSequence>(handle) }
}
