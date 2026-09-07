use crate::Counter;
use bridgerton::bridge;

#[bridge]
impl Counter {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn platform_value(&self) -> u32 {
        42
    }

    #[cfg(target_arch = "wasm32")]
    pub fn platform_value(&self) -> String {
        "web".into()
    }

    // Disabled signatures must never be inspected by the binding generator.
    #[cfg(any())]
    pub unsafe fn nonexistent<T>(&self, value: MissingType<T>) -> MissingType<T> {
        value
    }

    #[cfg_attr(all(), cfg_attr(all(), cfg(any())))]
    pub fn nested_disabled(&self, value: MissingType) -> MissingType {
        value
    }

    #[cfg_attr(all(), bridge(getter))]
    pub fn conditional_getter(&self) -> u32 {
        17
    }

    #[cfg_attr(all(), bridge(skip))]
    pub fn conditional_skip<T>(&self, value: T) -> T {
        value
    }
}
