//! Native type discovery. Rust resolves aliases and concrete generic arguments;
//! the generator only renders the resulting value shapes.
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    Scalar(&'static str),
    Named(String),
    Optional(Box<Type>),
    Array(Box<Type>),
    Map(Box<Type>, Box<Type>),
    Set(Box<Type>),
    Pair(Box<Type>, Box<Type>),
    Triple(Box<Type>, Box<Type>, Box<Type>),
}
impl Type {
    pub fn swift(&self) -> String {
        match self {
            Self::Scalar(s) => s.to_string(),
            Self::Named(s) => format!("`{s}`"),
            Self::Optional(t) => format!("Optional<{}>", t.swift()),
            Self::Array(t) => format!("Array<{}>", t.swift()),
            Self::Map(k, v) => format!("Dictionary<{}, {}>", k.swift(), v.swift()),
            Self::Set(t) => format!("Swift.Set<{}>", t.swift()),
            Self::Triple(a, b, c) => {
                format!("BridgeTriple<{}, {}, {}>", a.swift(), b.swift(), c.swift())
            }
            Self::Pair(a, b) => format!("BridgePair<{}, {}>", a.swift(), b.swift()),
        }
    }
    fn identifier(&self) -> String {
        self.swift()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect::<String>()
            .trim_matches('_')
            .to_string()
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub ty: Type,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variant {
    pub name: String,
    pub fields: Vec<Field>,
    pub named: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shape {
    Alias(Type),
    Record(Vec<Field>),
    Enum(Vec<Variant>),
}
#[derive(Clone, Debug, Default)]
pub struct Registry {
    problems: Vec<String>,
    names: BTreeMap<String, &'static str>,
    shapes: BTreeMap<&'static str, (String, Shape)>,
    errors: BTreeSet<String>,
    visiting: BTreeSet<&'static str>,
}
pub trait NativeType {
    fn native_type(registry: &mut Registry) -> Type;
    // Most values use the binary codec. Primitive returns can keep their direct
    // ABI without making aliases or application declarations handle that choice.
    fn return_value(self) -> Result<crate::native::BridgeResult, crate::Error>
    where
        Self: crate::value::Value,
    {
        crate::native::value(&self)
    }
    fn return_decoder() -> &'static str {
        "bridgeValue"
    }
}
impl Registry {
    fn register(&mut self, name: &str, rust: &'static str) {
        let reserved = name.starts_with("Bridge")
            || matches!(
                name,
                "String"
                    | "Bool"
                    | "Int"
                    | "UInt"
                    | "Int8"
                    | "UInt8"
                    | "Int16"
                    | "UInt16"
                    | "Int32"
                    | "UInt32"
                    | "Int64"
                    | "UInt64"
                    | "Float"
                    | "Double"
                    | "Optional"
                    | "Array"
                    | "Dictionary"
                    | "Set"
                    | "Void"
                    | "Data"
                    | "Date"
                    | "UUID"
                    | "Task"
            );
        if reserved {
            self.problems
                .push(format!("reserved Swift type name: {name} ({rust})"));
        }
        if let Some(previous) = self.names.insert(name.into(), rust)
            && previous != rust
        {
            self.problems.push(format!(
                "Swift type collision: {name} is used by {previous} and {rust}"
            ));
        }
    }
    pub fn validate(&self) -> Result<(), crate::Error> {
        if self.problems.is_empty() {
            Ok(())
        } else {
            Err(crate::Error::new(self.problems.join("\n")))
        }
    }
    pub fn error<T: NativeType>(&mut self) -> String {
        let ty = T::native_type(self);
        if let Type::Named(name) = &ty
            && let Some((_, Shape::Record(_) | Shape::Enum(_))) =
                self.names.get(name).and_then(|rust| self.shapes.get(rust))
        {
            self.errors.insert(name.clone());
            return ty.swift();
        }
        format!("BridgeFailure<{}>", ty.swift())
    }
    pub fn error_shape<T>(&mut self, name: &str, shape: impl FnOnce(&mut Self) -> Shape) -> Type {
        let ty = self.named::<T>(name, vec![], shape);
        self.errors.insert(name.into());
        ty
    }

    pub fn object<T>(&mut self, name: &str) -> Type {
        let rust = std::any::type_name::<T>();
        self.register(name, rust);
        Type::Named(name.into())
    }

    pub fn named<T>(
        &mut self,
        base: &str,
        arguments: Vec<Type>,
        shape: impl FnOnce(&mut Self) -> Shape,
    ) -> Type {
        let rust = std::any::type_name::<T>();
        let name = if arguments.is_empty() {
            base.to_owned()
        } else {
            format!(
                "{}_{}",
                base,
                arguments
                    .iter()
                    .map(Type::identifier)
                    .collect::<Vec<_>>()
                    .join("_")
            )
        };
        self.register(&name, rust);
        if self.visiting.insert(rust) {
            let value = shape(self);
            self.shapes.insert(rust, (name.clone(), value));
        }
        Type::Named(name)
    }
    pub fn merge(&mut self, other: Self) {
        self.errors.extend(other.errors);
        self.problems.extend(other.problems);
        for (name, rust) in other.names {
            self.register(&name, rust);
        }
        for (rust, value) in other.shapes {
            if let Some(previous) = self.shapes.insert(rust, value.clone())
                && previous != value
            {
                self.problems
                    .push(format!("inconsistent type metadata for {rust}"));
            }
        }
    }
    pub fn swift(&self) -> String {
        let mut swift: String = self
            .shapes
            .values()
            .map(|(name, shape)| render(self, name, shape))
            .collect();
        for name in &self.errors {
            swift += &format!("\nextension `{name}`: Swift.Error {{}}\n");
        }
        swift
    }

    // Follow only inline Swift storage. Collections and object references break
    // layout cycles; Optional, pairs, records, and aliases do not. Rust Box<T>
    // is erased in Swift, so its pointee participates in this analysis.
    fn reaches(
        &self,
        ty: &Type,
        target: &str,
        seen: &mut BTreeSet<String>,
        through_enums: bool,
    ) -> bool {
        match ty {
            Type::Named(name) => {
                if name == target {
                    return true;
                }
                if !seen.insert(name.clone()) {
                    return false;
                }
                match self.names.get(name).and_then(|rust| self.shapes.get(rust)) {
                    Some((_, Shape::Alias(ty))) => self.reaches(ty, target, seen, through_enums),
                    Some((_, Shape::Record(fields))) => fields
                        .iter()
                        .any(|f| self.reaches(&f.ty, target, seen, through_enums)),
                    Some((_, Shape::Enum(_))) if !through_enums => false,
                    Some((_, Shape::Enum(variants))) => variants
                        .iter()
                        .flat_map(|v| &v.fields)
                        .any(|f| self.reaches(&f.ty, target, seen, through_enums)),
                    None => false, // A bridged object is a Swift class reference.
                }
            }
            Type::Optional(ty) => self.reaches(ty, target, seen, through_enums),
            Type::Triple(a, b, c) => {
                self.reaches(a, target, seen, through_enums)
                    || self.reaches(b, target, seen, through_enums)
                    || self.reaches(c, target, seen, through_enums)
            }
            Type::Pair(a, b) => {
                self.reaches(a, target, seen, through_enums)
                    || self.reaches(b, target, seen, through_enums)
            }
            Type::Scalar(_) | Type::Array(_) | Type::Map(_, _) | Type::Set(_) => false,
        }
    }

    fn indirect(&self, name: &str, variant: &Variant) -> bool {
        variant
            .fields
            .iter()
            .any(|f| self.reaches(&f.ty, name, &mut BTreeSet::new(), true))
    }
}
fn label(s: &str) -> String {
    format!("`{}`", s.trim_start_matches("r#"))
}
fn render(registry: &Registry, name: &str, shape: &Shape) -> String {
    let mut out = String::new();
    let escaped = label(name);
    match shape {
        Shape::Alias(ty) => {
            out += &format!("public typealias {escaped} = {}\n", ty.swift());
        }
        Shape::Record(fields) => {
            out += &format!("\npublic struct {escaped}: Hashable, Sendable {{\n");
            for f in fields {
                if registry.reaches(&f.ty, name, &mut BTreeSet::new(), false) {
                    out += "    @BridgeIndirect\n";
                }
                out += &format!("    public var {}: {}\n", label(&f.name), f.ty.swift());
            }
            out += &format!(
                "    public init({}) {{\n",
                fields
                    .iter()
                    .map(|f| format!("{}: {}", label(&f.name), f.ty.swift()))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            for f in fields {
                out += &format!("        self.{0} = {0}\n", label(&f.name));
            }
            out += "    }\n}\n";
            out += &format!(
                "extension {escaped}: BridgeValue {{\n    internal static func bridgeRead(_ reader: inout BridgeReader) throws -> Self {{\n        try reader.nested {{ reader in Self({}) }}\n    }}\n    internal func bridgeWrite(_ writer: inout BridgeWriter) throws {{\n        try writer.nested {{ writer in\n",
                fields
                    .iter()
                    .map(|f| format!("{}: try reader.read()", f.name.trim_start_matches("r#")))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            for f in fields {
                out += &format!(
                    "            try self.{}.bridgeWrite(&writer)\n",
                    label(&f.name)
                );
            }
            out += "        }\n    }\n}\n";
        }
        Shape::Enum(variants) => {
            out += &format!("\npublic enum {escaped}: Hashable, Sendable {{\n");
            for v in variants {
                let indirect = if registry.indirect(name, v) {
                    "indirect "
                } else {
                    ""
                };
                out += &format!("    {indirect}case {}", label(&v.name));
                if !v.fields.is_empty() {
                    out += &format!(
                        "({})",
                        v.fields
                            .iter()
                            .map(|f| if v.named {
                                format!("{}: {}", label(&f.name), f.ty.swift())
                            } else {
                                f.ty.swift()
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                out += "\n";
            }
            out += &format!(
                "}}\nextension {escaped}: BridgeValue {{\n    internal static func bridgeRead(_ reader: inout BridgeReader) throws -> Self {{\n        try reader.nested {{ reader in\n            switch try UInt32.bridgeRead(&reader) {{\n"
            );
            for (i, v) in variants.iter().enumerate() {
                out += &format!("            case {}: return .{}", i + 1, label(&v.name));
                if !v.fields.is_empty() {
                    out += &format!(
                        "({})",
                        v.fields
                            .iter()
                            .map(|f| if v.named {
                                format!("{}: try reader.read()", f.name.trim_start_matches("r#"))
                            } else {
                                "try reader.read()".into()
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                out += "\n";
            }
            out += "            default: throw BridgeError(description: \"invalid enum tag\")\n            }\n        }\n    }\n    internal func bridgeWrite(_ writer: inout BridgeWriter) throws {\n        try writer.nested { writer in\n            switch self {\n";
            for (i, v) in variants.iter().enumerate() {
                out += &format!(
                    "            case {}.{}",
                    if v.fields.is_empty() { "" } else { "let " },
                    label(&v.name)
                );
                if !v.fields.is_empty() {
                    out += &format!(
                        "({})",
                        (0..v.fields.len())
                            .map(|i| format!("field_{i}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                out += &format!(
                    ":\n                try UInt32({}).bridgeWrite(&writer)\n",
                    i + 1
                );
                for i in 0..v.fields.len() {
                    out += &format!("                try field_{i}.bridgeWrite(&writer)\n");
                }
            }
            out += "            }\n        }\n    }\n}\n";
        }
    }
    out
}
macro_rules! scalar {
    ($($rust:ty => $swift:literal),* $(,)?)=>{$(impl crate::native::CallbackArguments for $rust { crate::__native_callback_value!(); } impl NativeType for $rust {fn native_type(_: &mut Registry)->Type {Type::Scalar($swift)}} impl crate::native::NativeReturn for $rust { crate::__native_value_return!(); }
impl crate::native::NativeArgument for $rust { crate::__native_value_argument!(); }
impl crate::native::NativeOptionalArgument for $rust { crate::__native_optional_value_argument!(); })*};
}
scalar!(u8=>"UInt8",u16=>"UInt16",u64=>"UInt64",usize=>"UInt64",i8=>"Int8",i16=>"Int16",i32=>"Int32",i64=>"Int64",f32=>"Float",f64=>"Double",bool=>"Bool");
impl NativeType for () {
    fn native_type(_: &mut Registry) -> Type {
        Type::Scalar("BridgeUnit")
    }
}
impl crate::native::NativeReturn for () {
    crate::__native_value_return!();
}
impl crate::native::NativeArgument for () {
    crate::__native_value_argument!();
}
impl crate::native::NativeOptionalArgument for () {
    crate::__native_optional_value_argument!();
}
impl<T: NativeType> NativeType for Option<T> {
    fn return_decoder() -> &'static str {
        "bridgeReturn"
    }
    fn native_type(r: &mut Registry) -> Type {
        Type::Optional(Box::new(T::native_type(r)))
    }
}
impl<T: NativeType> NativeType for Vec<T> {
    fn return_decoder() -> &'static str {
        "bridgeReturn"
    }
    fn native_type(r: &mut Registry) -> Type {
        Type::Array(Box::new(T::native_type(r)))
    }
}
impl<K: NativeType, V: NativeType> NativeType for BTreeMap<K, V> {
    fn native_type(r: &mut Registry) -> Type {
        Type::Map(Box::new(K::native_type(r)), Box::new(V::native_type(r)))
    }
}
impl<T: NativeType> NativeType for BTreeSet<T> {
    fn native_type(r: &mut Registry) -> Type {
        Type::Set(Box::new(T::native_type(r)))
    }
}
impl<A: NativeType, B: NativeType> NativeType for (A, B) {
    fn native_type(r: &mut Registry) -> Type {
        Type::Pair(Box::new(A::native_type(r)), Box::new(B::native_type(r)))
    }
}
impl<T: NativeType> NativeType for Box<T> {
    fn return_decoder() -> &'static str {
        T::return_decoder()
    }
    fn native_type(r: &mut Registry) -> Type {
        T::native_type(r)
    }
}
impl NativeType for chrono::DateTime<chrono::Utc> {
    fn native_type(_: &mut Registry) -> Type {
        Type::Scalar("BridgeTimestamp")
    }
}

macro_rules! direct_return {
    ($rust:ty, $swift:literal, $decoder:literal) => {
        impl crate::native::CallbackArguments for $rust {
            crate::__native_callback_value!();
        }
        impl crate::native::NativeReturn for $rust {
            crate::__native_value_return!();
        }
        impl crate::native::NativeArgument for $rust {
            crate::__native_value_argument!();
        }
        impl crate::native::NativeOptionalArgument for $rust {
            crate::__native_optional_value_argument!();
        }
        impl NativeType for $rust {
            fn native_type(_: &mut Registry) -> Type {
                Type::Scalar($swift)
            }
            fn return_value(self) -> Result<crate::native::BridgeResult, crate::Error> {
                Ok(crate::native::IntoResult::into_result(self))
            }
            fn return_decoder() -> &'static str {
                $decoder
            }
        }
    };
}
direct_return!(u32, "UInt32", "bridgeNumber");
direct_return!(String, "String", "bridgeString");

impl<K: crate::value::Value + NativeType + Ord, V: crate::value::Value + NativeType>
    crate::native::NativeReturn for BTreeMap<K, V>
{
    crate::__native_value_return!();
}
impl<K: crate::value::Value + NativeType + Ord, V: crate::value::Value + NativeType>
    crate::native::NativeArgument for BTreeMap<K, V>
{
    crate::__native_value_argument!();
}
impl<K: crate::value::Value + NativeType + Ord, V: crate::value::Value + NativeType>
    crate::native::NativeOptionalArgument for BTreeMap<K, V>
{
    crate::__native_optional_value_argument!();
}
impl<T: crate::value::Value + NativeType + Ord> crate::native::NativeReturn for BTreeSet<T> {
    crate::__native_value_return!();
}
impl<T: crate::value::Value + NativeType + Ord> crate::native::NativeArgument for BTreeSet<T> {
    crate::__native_value_argument!();
}
impl<T: crate::value::Value + NativeType + Ord> crate::native::NativeOptionalArgument
    for BTreeSet<T>
{
    crate::__native_optional_value_argument!();
}
impl<A: crate::value::Value + NativeType, B: crate::value::Value + NativeType>
    crate::native::NativeReturn for (A, B)
{
    crate::__native_value_return!();
}
impl<A: crate::value::Value + NativeType, B: crate::value::Value + NativeType>
    crate::native::NativeArgument for (A, B)
{
    crate::__native_value_argument!();
}
impl<A: crate::value::Value + NativeType, B: crate::value::Value + NativeType>
    crate::native::NativeOptionalArgument for (A, B)
{
    crate::__native_optional_value_argument!();
}
impl crate::native::NativeReturn for chrono::DateTime<chrono::Utc> {
    crate::__native_value_return!();
}
impl crate::native::NativeArgument for chrono::DateTime<chrono::Utc> {
    crate::__native_value_argument!();
}
impl crate::native::NativeOptionalArgument for chrono::DateTime<chrono::Utc> {
    crate::__native_optional_value_argument!();
}

impl<A: NativeType, B: NativeType, C: NativeType> NativeType for (A, B, C) {
    fn native_type(r: &mut Registry) -> Type {
        Type::Triple(
            Box::new(A::native_type(r)),
            Box::new(B::native_type(r)),
            Box::new(C::native_type(r)),
        )
    }
}
impl<
    A: crate::value::Value + NativeType,
    B: crate::value::Value + NativeType,
    C: crate::value::Value + NativeType,
> crate::native::NativeReturn for (A, B, C)
{
    crate::__native_value_return!();
}
impl<
    A: crate::value::Value + NativeType,
    B: crate::value::Value + NativeType,
    C: crate::value::Value + NativeType,
> crate::native::NativeArgument for (A, B, C)
{
    crate::__native_value_argument!();
}
impl<
    A: crate::value::Value + NativeType,
    B: crate::value::Value + NativeType,
    C: crate::value::Value + NativeType,
> crate::native::NativeOptionalArgument for (A, B, C)
{
    crate::__native_optional_value_argument!();
}

impl<T: crate::native::NativeReturn<Success = T> + NativeType + 'static>
    crate::native::CallbackArguments for Vec<T>
{
    crate::__native_callback_value!();
}
impl<T: crate::native::NativeReturn<Success = T> + NativeType + 'static>
    crate::native::CallbackArguments for Option<T>
{
    crate::__native_callback_value!();
}
impl<T: crate::native::NativeReturn<Success = T> + NativeType + 'static>
    crate::native::CallbackArguments for Box<T>
{
    crate::__native_callback_value!();
}
impl<K: crate::value::Value + NativeType + Ord, V: crate::value::Value + NativeType>
    crate::native::CallbackArguments for BTreeMap<K, V>
{
    crate::__native_callback_value!();
}
impl<T: crate::value::Value + NativeType + Ord> crate::native::CallbackArguments for BTreeSet<T> {
    crate::__native_callback_value!();
}
impl crate::native::CallbackArguments for chrono::DateTime<chrono::Utc> {
    crate::__native_callback_value!();
}

impl<T: crate::value::Value + NativeType> crate::native::NativeArgument for Vec<T> {
    crate::__native_value_argument!();
}
impl<T: crate::value::Value + NativeType> crate::native::NativeOptionalArgument for Vec<T> {
    crate::__native_optional_value_argument!();
}
impl<T: crate::value::Value + NativeType> crate::native::NativeArgument for Box<T> {
    crate::__native_value_argument!();
}
impl<T: crate::value::Value + NativeType> crate::native::NativeOptionalArgument for Box<T> {
    crate::__native_optional_value_argument!();
}
impl<T: crate::value::Value + NativeType> crate::native::NativeOptionalArgument for Option<T> {
    crate::__native_optional_value_argument!();
}
