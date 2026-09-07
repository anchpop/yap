use super::Counter;
use bridgerton::bridge;

// A separate module needs no part label or generator registration on either platform.
#[bridge]
impl Counter {
    pub fn label(&self) -> String {
        format!("Yap 語 — {}", self.value.get())
    }
}
