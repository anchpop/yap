#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(not(target_arch = "wasm32"))]
use uuid::Uuid;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["self", "crypto"])]
    fn randomUUID() -> String;
}

pub fn generate_uuid() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        randomUUID()
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        Uuid::new_v4().to_string()
    }
}
