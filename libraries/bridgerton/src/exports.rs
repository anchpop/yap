//! Generated declarations register themselves across crates and impl blocks.
//! Only metadata factories are shared; application objects remain thread-confined.
use crate::{Error, native::Definition};
use std::collections::{BTreeMap, BTreeSet};

type Members = &'static [(&'static str, bool)];

#[doc(hidden)]
pub struct Export {
    function: bool,
    rust_type: fn() -> &'static str,
    name: &'static str,
    members: Option<Members>,
    definition: fn() -> Definition,
}
impl Export {
    pub const fn object<T>(name: &'static str, definition: fn() -> Definition) -> Self {
        Self {
            function: false,
            rust_type: std::any::type_name::<T>,
            name,
            members: None,
            definition,
        }
    }
    pub const fn function<T>(name: &'static str, definition: fn() -> Definition) -> Self {
        Self {
            function: true,
            rust_type: std::any::type_name::<T>,
            name,
            members: None,
            definition,
        }
    }
    pub const fn methods<T>(
        name: &'static str,
        members: Members,
        definition: fn() -> Definition,
    ) -> Self {
        Self {
            function: false,
            rust_type: std::any::type_name::<T>,
            name,
            members: Some(members),
            definition,
        }
    }
}
inventory::collect!(Export);

/// Collect all linked exports, independent of registration/link order.
pub fn definition() -> Result<Definition, Error> {
    collect(inventory::iter::<Export>.into_iter())
}

fn collect<'a>(exports: impl Iterator<Item = &'a Export>) -> Result<Definition, Error> {
    let mut objects = BTreeMap::new();
    let mut names = BTreeMap::new();
    let mut methods = Vec::new();
    let mut functions = Vec::new();
    for export in exports {
        let rust = (export.rust_type)();
        if export.name.starts_with("Bridge") || export.name.starts_with("__bridgerton") {
            return Err(Error::new(format!(
                "reserved Swift export name: {}",
                export.name
            )));
        }
        if export.function {
            if names.insert(export.name, rust).is_some() {
                return Err(Error::new(format!(
                    "duplicate Swift export name: {}",
                    export.name
                )));
            }
            functions.push(export);
        } else if export.members.is_some() {
            methods.push(export);
        } else {
            if objects.insert(rust, export).is_some() {
                return Err(Error::new(format!("object declared twice: {rust}")));
            }
            if names.insert(export.name, rust).is_some() {
                return Err(Error::new(format!(
                    "duplicate Swift object name: {}",
                    export.name
                )));
            }
        }
    }
    let mut fragments: Vec<_> = functions.iter().map(|e| (e.definition)()).collect();
    let mut seen = BTreeSet::new();
    let mut constructors = BTreeSet::new();
    for export in methods {
        let rust = (export.rust_type)();
        let Some(object) = objects.get(rust) else {
            return Err(Error::new(format!(
                "exported impl has no bridged object declaration: {rust}"
            )));
        };
        if object.name != export.name {
            return Err(Error::new(format!(
                "impl and object use different Swift names for {rust}"
            )));
        }
        for &(method, constructor) in export.members.unwrap() {
            if method.starts_with("__bridgerton") || method == "bridgeReceive" {
                return Err(Error::new(format!(
                    "reserved Swift method name: {rust}::{method}"
                )));
            }
            if !seen.insert((rust, method)) {
                return Err(Error::new(format!(
                    "duplicate exported method: {rust}::{method}"
                )));
            }
            if constructor && !constructors.insert(rust) {
                return Err(Error::new(format!(
                    "multiple constructors exported for {rust}"
                )));
            }
        }
        fragments.push((export.definition)());
    }
    // Different incremental compilations may register fragments in any order.
    fragments.sort_by(|a, b| (&a.swift, &a.header).cmp(&(&b.swift, &b.header)));
    let mut result = Definition {
        header: String::new(),
        swift: String::new(),
        types: Default::default(),
    };
    for fragment in objects.values().map(|e| (e.definition)()).chain(fragments) {
        result.header.push_str(&fragment.header);
        result.swift.push_str(&fragment.swift);
        result.types.merge(fragment.types);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Object;
    struct Other;

    fn class() -> Definition {
        Definition {
            header: "free;".into(),
            swift: "class Object {}".into(),
            types: Default::default(),
        }
    }
    fn first() -> Definition {
        Definition {
            header: "first;".into(),
            swift: "extension Object { first }".into(),
            types: Default::default(),
        }
    }
    fn second() -> Definition {
        Definition {
            header: "second;".into(),
            swift: "extension Object { second }".into(),
            types: Default::default(),
        }
    }

    #[test]
    fn split_impls_have_one_owner_and_deterministic_output() {
        let entries = [
            Export::methods::<Object>("Object", &[("second", false)], second),
            Export::object::<Object>("Object", class),
            Export::methods::<Object>("Object", &[("first", true)], first),
        ];
        let forward = collect(entries.iter()).unwrap();
        let reverse = collect(entries.iter().rev()).unwrap();
        assert_eq!(forward.header, "free;first;second;");
        assert_eq!(forward.header, reverse.header);
        assert_eq!(forward.swift, reverse.swift);
        assert_eq!(forward.swift.matches("class Object").count(), 1);
    }

    fn error(entries: &[Export]) -> String {
        match collect(entries.iter()) {
            Err(error) => error.to_string(),
            Ok(_) => panic!("expected invalid exports to fail"),
        }
    }

    #[test]
    fn impl_requires_an_object_declaration() {
        assert!(
            error(&[Export::methods::<Object>("Object", &[], first)])
                .contains("no bridged object declaration")
        );
    }

    #[test]
    fn constructors_and_methods_are_checked_across_impls() {
        assert!(
            error(&[
                Export::object::<Object>("Object", class),
                Export::methods::<Object>("Object", &[("new", true)], first),
                Export::methods::<Object>("Object", &[("create", true)], second),
            ])
            .contains("multiple constructors")
        );
        assert!(
            error(&[
                Export::object::<Object>("Object", class),
                Export::methods::<Object>("Object", &[("method", false)], first),
                Export::methods::<Object>("Object", &[("method", false)], second),
            ])
            .contains("duplicate exported method")
        );
    }

    #[test]
    fn conflicting_object_names_fail_generation() {
        assert!(
            error(&[
                Export::object::<Object>("Object", class),
                Export::object::<Other>("Object", class),
            ])
            .contains("duplicate Swift object name")
        );
        assert!(
            error(&[
                Export::object::<Object>("Object", class),
                Export::methods::<Object>("Different", &[], first),
            ])
            .contains("different Swift names")
        );
    }
}
