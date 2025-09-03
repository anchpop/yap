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

pub fn get_uuid() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        randomUUID()
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        Uuid::new_v4().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_uuid() {
        let uuid1 = get_uuid();
        let uuid2 = get_uuid();

        assert_ne!(uuid1, uuid2);
        assert_eq!(uuid1.len(), 36);
        assert!(uuid1.chars().filter(|&c| c == '-').count() == 4);
    }
}
