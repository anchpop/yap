use weapon::supabase::SupabaseConfig;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn supabase_config() -> SupabaseConfig {
    SupabaseConfig {
        supabase_url: "https://eearwzqotpfoderpfrqx.supabase.co".to_string(),
        supabase_anon_key: "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6ImVlYXJ3enFvdHBmb2RlcnBmcnF4Iiwicm9sZSI6ImFub24iLCJpYXQiOjE3NDgyMTUwOTIsImV4cCI6MjA2Mzc5MTA5Mn0.BmnDrHtD-THaSLHO9VE2X-PO6B-z9OkbxzjeIinN6b8".to_string(),
    }
}
