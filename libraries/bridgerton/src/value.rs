//! Owned value encoding: big-endian integers, length-prefixed UTF-8/sequences,
//! one-byte option tags, and one-based enum tags, following UniFFI's approach.
//! No pointers, object handles, or thread requirements are part of this format.
use crate::Error;

pub const MAX_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_ITEMS: usize = 65_536;
const MAX_DEPTH: usize = 64;

pub trait Value: Sized {
    fn read(reader: &mut Reader<'_>) -> Result<Self, Error>;
    fn write(&self, writer: &mut Writer) -> Result<(), Error>;
}

pub fn encode<T: Value>(value: &T) -> Result<Vec<u8>, Error> {
    encode_with(|writer| value.write(writer))
}

#[doc(hidden)]
pub fn encode_with(body: impl FnOnce(&mut Writer) -> Result<(), Error>) -> Result<Vec<u8>, Error> {
    let mut writer = Writer {
        bytes: Vec::new(),
        depth: 0,
        remaining_items: MAX_ITEMS,
    };
    body(&mut writer)?;
    Ok(writer.bytes)
}

pub fn decode<T: Value>(bytes: &[u8]) -> Result<T, Error> {
    if bytes.len() > MAX_BYTES {
        return Err(Error::new("value exceeds byte limit"));
    }
    let mut reader = Reader {
        bytes,
        depth: 0,
        remaining_items: MAX_ITEMS,
    };
    let value = T::read(&mut reader)?;
    if !reader.bytes.is_empty() {
        return Err(Error::new("trailing value bytes"));
    }
    Ok(value)
}

pub struct Reader<'a> {
    bytes: &'a [u8],
    depth: usize,
    remaining_items: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], Error> {
        if count > self.bytes.len() {
            return Err(Error::new("truncated value"));
        }
        let (head, tail) = self.bytes.split_at(count);
        self.bytes = tail;
        Ok(head)
    }
    pub fn nested<T>(
        &mut self,
        body: impl FnOnce(&mut Self) -> Result<T, Error>,
    ) -> Result<T, Error> {
        if self.depth == MAX_DEPTH {
            return Err(Error::new("value exceeds nesting limit"));
        }
        self.depth += 1;
        let result = body(self);
        self.depth -= 1;
        result
    }
    fn length(&mut self, limit: usize) -> Result<usize, Error> {
        let count = u32::read(self)? as usize;
        if count > limit {
            return Err(Error::new("invalid value length"));
        }
        Ok(count)
    }
}

pub struct Writer {
    bytes: Vec<u8>,
    depth: usize,
    remaining_items: usize,
}
impl Writer {
    fn put(&mut self, bytes: &[u8]) -> Result<(), Error> {
        if bytes.len() > MAX_BYTES - self.bytes.len() {
            return Err(Error::new("value exceeds byte limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    pub fn nested<T>(
        &mut self,
        body: impl FnOnce(&mut Self) -> Result<T, Error>,
    ) -> Result<T, Error> {
        if self.depth == MAX_DEPTH {
            return Err(Error::new("value exceeds nesting limit"));
        }
        self.depth += 1;
        let result = body(self);
        self.depth -= 1;
        result
    }
    fn length(&mut self, count: usize, limit: usize) -> Result<(), Error> {
        if count > limit {
            return Err(Error::new("invalid value length"));
        }
        (count as u32).write(self)
    }
}

impl Value for u32 {
    fn read(reader: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(Self::from_be_bytes(reader.take(4)?.try_into().unwrap()))
    }
    fn write(&self, writer: &mut Writer) -> Result<(), Error> {
        writer.put(&self.to_be_bytes())
    }
}
impl Value for bool {
    fn read(reader: &mut Reader<'_>) -> Result<Self, Error> {
        match reader.take(1)?[0] {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::new("invalid boolean tag")),
        }
    }
    fn write(&self, writer: &mut Writer) -> Result<(), Error> {
        writer.put(&[u8::from(*self)])
    }
}
impl Value for String {
    fn read(reader: &mut Reader<'_>) -> Result<Self, Error> {
        let count = reader.length(MAX_BYTES)?;
        String::from_utf8(reader.take(count)?.to_vec()).map_err(|_| Error::new("invalid UTF-8"))
    }
    fn write(&self, writer: &mut Writer) -> Result<(), Error> {
        writer.length(self.len(), MAX_BYTES)?;
        writer.put(self.as_bytes())
    }
}
impl<T: Value> Value for Option<T> {
    fn read(reader: &mut Reader<'_>) -> Result<Self, Error> {
        reader.nested(|reader| {
            if bool::read(reader)? {
                Ok(Some(T::read(reader)?))
            } else {
                Ok(None)
            }
        })
    }
    fn write(&self, writer: &mut Writer) -> Result<(), Error> {
        writer.nested(|writer| {
            self.is_some().write(writer)?;
            if let Some(value) = self {
                value.write(writer)?;
            }
            Ok(())
        })
    }
}
impl<T: Value> Value for Vec<T> {
    fn read(reader: &mut Reader<'_>) -> Result<Self, Error> {
        reader.nested(|reader| {
            let count = reader.length(reader.remaining_items)?;
            reader.remaining_items -= count;
            // Don't reserve attacker-controlled capacity before validating elements.
            (0..count).map(|_| T::read(reader)).collect()
        })
    }
    fn write(&self, writer: &mut Writer) -> Result<(), Error> {
        writer.nested(|writer| {
            writer.length(self.len(), writer.remaining_items)?;
            writer.remaining_items -= self.len();
            for value in self {
                value.write(writer)?;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_bytes_and_invalid_inputs() {
        let value = Some(vec!["語".to_string()]);
        let bytes = [1, 0, 0, 0, 1, 0, 0, 0, 3, 0xe8, 0xaa, 0x9e];
        assert_eq!(encode(&value).unwrap(), bytes);
        assert_eq!(decode::<Option<Vec<String>>>(&bytes).unwrap(), value);
        for end in 0..bytes.len() {
            assert!(decode::<Option<Vec<String>>>(&bytes[..end]).is_err());
        }
        assert!(decode::<bool>(&[2]).is_err());
        assert!(decode::<Option<u32>>(&[2]).is_err());
        assert!(decode::<String>(&[0, 0, 0, 1, 0xff]).is_err());
        assert!(decode::<Vec<u32>>(&[0xff; 4]).is_err());
        assert!(decode::<String>(&[0xff; 4]).is_err());
        assert!(decode::<u32>(&[0; 5]).is_err());
        assert!(encode(&vec![false; MAX_ITEMS + 1]).is_err());
        assert!(encode(&vec![vec![false; 256]; 256]).is_err());
    }
    #[test]
    fn numeric_and_collection_wire_vectors() {
        use std::collections::{BTreeMap, BTreeSet};
        assert_eq!(
            encode(&usize::MAX).unwrap(),
            (usize::MAX as u64).to_be_bytes()
        );
        assert_eq!(encode(&-2i16).unwrap(), [255, 254]);
        for bits in [0, 1 << 63, 0x7ff0_0000_0000_0000, 0x7ff8_0000_0000_1234] {
            let value = f64::from_bits(bits);
            assert_eq!(
                decode::<f64>(&encode(&value).unwrap()).unwrap().to_bits(),
                bits
            );
        }
        let map = BTreeMap::from([(1u32, Some(2u16))]);
        let bytes = [0, 0, 0, 1, 0, 0, 0, 1, 1, 0, 2];
        assert_eq!(encode(&map).unwrap(), bytes);
        assert_eq!(decode::<BTreeMap<u32, Option<u16>>>(&bytes).unwrap(), map);
        let duplicate_keys = [0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0];
        assert!(decode::<BTreeMap<u32, Option<u16>>>(&duplicate_keys).is_err());
        assert!(decode::<BTreeSet<u32>>(&[0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0, 1]).is_err());
        assert!(decode::<BTreeMap<u32, u32>>(&[255; 4]).is_err());
        assert_eq!(encode(&(1u8, -2i16)).unwrap(), [1, 255, 254]);
    }

    #[test]
    fn timestamps_are_lossless_and_validate_ranges() {
        let value = chrono::DateTime::from_timestamp(-1, 123_456_789).unwrap();
        let bytes = encode(&value).unwrap();
        assert_eq!(
            bytes,
            [255, 255, 255, 255, 255, 255, 255, 255, 7, 91, 205, 21]
        );
        assert_eq!(
            decode::<chrono::DateTime<chrono::Utc>>(&bytes).unwrap(),
            value
        );
        assert!(decode::<chrono::DateTime<chrono::Utc>>(&[255; 12]).is_err());
    }

    #[test]
    fn nesting_limit_is_enforced_on_read_and_write() {
        struct Recursive;
        impl Value for Recursive {
            fn read(r: &mut Reader<'_>) -> Result<Self, Error> {
                r.nested(Self::read)
            }
            fn write(&self, w: &mut Writer) -> Result<(), Error> {
                w.nested(|w| self.write(w))
            }
        }
        assert!(decode::<Recursive>(&[]).is_err());
        assert!(encode(&Recursive).is_err());
    }
}

impl Value for () {
    fn read(_: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(())
    }
    fn write(&self, _: &mut Writer) -> Result<(), Error> {
        Ok(())
    }
}
impl<A: Value, B: Value> Value for (A, B) {
    fn read(r: &mut Reader<'_>) -> Result<Self, Error> {
        Ok((A::read(r)?, B::read(r)?))
    }
    fn write(&self, w: &mut Writer) -> Result<(), Error> {
        self.0.write(w)?;
        self.1.write(w)
    }
}
macro_rules! integer_value {
    ($ty:ty) => {
        impl Value for $ty {
            fn read(r: &mut Reader<'_>) -> Result<Self, Error> {
                Ok(Self::from_be_bytes(
                    r.take(std::mem::size_of::<Self>())?.try_into().unwrap(),
                ))
            }
            fn write(&self, w: &mut Writer) -> Result<(), Error> {
                w.put(&self.to_be_bytes())
            }
        }
    };
}
integer_value!(i32);
integer_value!(u64);

integer_value!(u8);
integer_value!(u16);
integer_value!(i8);
integer_value!(i16);
integer_value!(i64);
impl Value for usize {
    fn read(r: &mut Reader<'_>) -> Result<Self, Error> {
        Self::try_from(u64::read(r)?).map_err(|_| Error::new("integer out of range"))
    }
    fn write(&self, w: &mut Writer) -> Result<(), Error> {
        (*self as u64).write(w)
    }
}
impl Value for f64 {
    fn read(r: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(Self::from_bits(u64::read(r)?))
    }
    fn write(&self, w: &mut Writer) -> Result<(), Error> {
        self.to_bits().write(w)
    }
}
impl Value for f32 {
    fn read(r: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(Self::from_bits(u32::read(r)?))
    }
    fn write(&self, w: &mut Writer) -> Result<(), Error> {
        self.to_bits().write(w)
    }
}
impl<T: Value> Value for Box<T> {
    fn read(r: &mut Reader<'_>) -> Result<Self, Error> {
        T::read(r).map(Box::new)
    }
    fn write(&self, w: &mut Writer) -> Result<(), Error> {
        (**self).write(w)
    }
}
impl<K: Value + Ord, V: Value> Value for std::collections::BTreeMap<K, V> {
    fn read(r: &mut Reader<'_>) -> Result<Self, Error> {
        r.nested(|r| {
            let count = r.length(r.remaining_items)?;
            r.remaining_items -= count;
            let mut result = Self::new();
            for _ in 0..count {
                let k = K::read(r)?;
                let v = V::read(r)?;
                if result.insert(k, v).is_some() {
                    return Err(Error::new("duplicate map key"));
                }
            }
            Ok(result)
        })
    }
    fn write(&self, w: &mut Writer) -> Result<(), Error> {
        w.nested(|w| {
            w.length(self.len(), w.remaining_items)?;
            w.remaining_items -= self.len();
            for (k, v) in self {
                k.write(w)?;
                v.write(w)?;
            }
            Ok(())
        })
    }
}
impl<T: Value + Ord> Value for std::collections::BTreeSet<T> {
    fn read(r: &mut Reader<'_>) -> Result<Self, Error> {
        let values = Vec::<T>::read(r)?;
        let count = values.len();
        let result: Self = values.into_iter().collect();
        if result.len() != count {
            Err(Error::new("duplicate set element"))
        } else {
            Ok(result)
        }
    }
    fn write(&self, w: &mut Writer) -> Result<(), Error> {
        w.nested(|w| {
            w.length(self.len(), w.remaining_items)?;
            w.remaining_items -= self.len();
            for v in self {
                v.write(w)?;
            }
            Ok(())
        })
    }
}
impl Value for chrono::DateTime<chrono::Utc> {
    fn read(r: &mut Reader<'_>) -> Result<Self, Error> {
        let seconds = i64::read(r)?;
        let nanos = u32::read(r)?;
        Self::from_timestamp(seconds, nanos).ok_or_else(|| Error::new("invalid timestamp"))
    }
    fn write(&self, w: &mut Writer) -> Result<(), Error> {
        self.timestamp().write(w)?;
        self.timestamp_subsec_nanos().write(w)
    }
}

impl<A: Value, B: Value, C: Value> Value for (A, B, C) {
    fn read(r: &mut Reader<'_>) -> Result<Self, Error> {
        Ok((A::read(r)?, B::read(r)?, C::read(r)?))
    }
    fn write(&self, w: &mut Writer) -> Result<(), Error> {
        self.0.write(w)?;
        self.1.write(w)?;
        self.2.write(w)
    }
}
