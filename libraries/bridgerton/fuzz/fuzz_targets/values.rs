#![no_main]
use bridgerton::value::{Value, decode, encode};
use libfuzzer_sys::fuzz_target;

type Card = (u64, String, (Vec<String>, Option<(u32, bool)>));

fn roundtrip<T: Value + PartialEq + std::fmt::Debug>(bytes: &[u8]) {
    if let Ok(value) = decode::<T>(bytes) {
        let encoded = encode(&value).expect("decoded value must fit the same limits");
        assert_eq!(decode::<T>(&encoded).unwrap(), value);
    }
}
fuzz_target!(|bytes: &[u8]| {
    roundtrip::<Card>(bytes);
    roundtrip::<Vec<Option<Card>>>(bytes);
    roundtrip::<Vec<Vec<Option<String>>>>(bytes);
    roundtrip::<std::collections::BTreeMap<String, Vec<u64>>>(bytes);
});
