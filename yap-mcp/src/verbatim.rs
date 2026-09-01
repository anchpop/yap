//! [`Verbatim`]: a tool parameter that echoes an object from an earlier tool
//! result back to the server.
//!
//! Typing such parameters as `serde_json::Value` gives them a schema with no
//! `type`, and at least one MCP client (claude.ai) then serializes the object
//! to a JSON *string* before it reaches us, so the server sees
//! `"{\"type\":\"WrittenGram\",...}"` instead of the object. `Verbatim<T>`
//! closes that off in both directions: its schema is `T`'s real schema, and
//! its deserializer still accepts a JSON-encoded string, parsing it first.

use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::de::{Deserialize, DeserializeOwned, Deserializer, Error as _};
use serde::ser::{Serialize, Serializer};

/// A value passed back exactly as an earlier tool result produced it.
#[derive(Debug, Clone)]
pub struct Verbatim<T>(pub T);

impl<T> std::ops::Deref for Verbatim<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: DeserializeOwned> Verbatim<T> {
    /// Parse a value that is either the object itself or the object
    /// JSON-encoded inside a string.
    pub fn from_value(value: serde_json::Value) -> Result<Self, serde_json::Error> {
        let direct = match serde_json::from_value::<T>(value.clone()) {
            Ok(inner) => return Ok(Verbatim(inner)),
            Err(e) => e,
        };
        if let serde_json::Value::String(text) = &value
            && let Ok(unwrapped) = serde_json::from_str::<serde_json::Value>(text)
            && !unwrapped.is_string()
        {
            return serde_json::from_value(unwrapped).map(Verbatim);
        }
        Err(direct)
    }
}

impl<T: Serialize> Serialize for Verbatim<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de, T: DeserializeOwned> Deserialize<'de> for Verbatim<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        Self::from_value(value).map_err(|e| {
            D::Error::custom(format!(
                "{e} — pass the object exactly as returned by the earlier tool call"
            ))
        })
    }
}

impl<T: JsonSchema> JsonSchema for Verbatim<T> {
    fn schema_name() -> Cow<'static, str> {
        T::schema_name()
    }

    fn schema_id() -> Cow<'static, str> {
        T::schema_id()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        T::json_schema(generator)
    }

    /// Inline `T`'s schema at the parameter itself rather than behind a
    /// `$ref`, so every client sees the parameter's type without chasing
    /// `$defs`. Nested types still go through `$defs`.
    fn inline_schema() -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize, JsonSchema, Debug, PartialEq)]
    struct Point {
        x: i32,
    }

    #[test]
    fn accepts_the_object() {
        let v: Verbatim<Point> = serde_json::from_value(serde_json::json!({"x": 1})).unwrap();
        assert_eq!(v.0, Point { x: 1 });
    }

    #[test]
    fn accepts_the_object_json_encoded_in_a_string() {
        let v: Verbatim<Point> = serde_json::from_value(serde_json::json!("{\"x\": 2}")).unwrap();
        assert_eq!(v.0, Point { x: 2 });
    }

    #[test]
    fn reports_the_direct_error_for_garbage() {
        let err = serde_json::from_value::<Verbatim<Point>>(serde_json::json!("nope"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("expected struct Point"), "{err}");
        assert!(err.contains("exactly as returned"), "{err}");
    }

    #[test]
    fn schema_is_a_typed_object() {
        let schema = schemars::schema_for!(Verbatim<Point>);
        let value = serde_json::to_value(schema).unwrap();
        assert_eq!(value["type"], "object");
    }
}
