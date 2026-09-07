//! Native layout tests use existing-style data annotations so TS parsing is independent.
use crate::Counter;
use bridgerton::bridge;
use std::collections::{BTreeMap, BTreeSet};

#[bridge(transparent)]
#[derive(bridgerton::serde::Serialize, bridgerton::serde::Deserialize)]
#[serde(crate = "bridgerton::serde")]
#[derive(Clone, Debug, PartialEq)]
pub enum Tree {
    Leaf(u32),
    Next(Box<Tree>),
    Record(TreeRecord),
    Children(Vec<Tree>),
    Pair(Box<(Tree, bool)>),
}

#[bridge(transparent)]
#[derive(bridgerton::serde::Serialize, bridgerton::serde::Deserialize)]
#[serde(crate = "bridgerton::serde")]
#[derive(Clone, Debug, PartialEq)]
pub struct TreeRecord {
    pub child: Option<Box<Tree>>,
}

#[bridge(transparent)]
#[derive(bridgerton::serde::Serialize, bridgerton::serde::Deserialize)]
#[serde(crate = "bridgerton::serde")]
#[derive(Clone, Debug, PartialEq)]
pub enum MutualA {
    End,
    Next(Box<MutualB>),
}

#[bridge(transparent)]
#[derive(bridgerton::serde::Serialize, bridgerton::serde::Deserialize)]
#[serde(crate = "bridgerton::serde")]
#[derive(Clone, Debug, PartialEq)]
pub enum MutualB {
    End,
    Next(Box<MutualA>),
}

#[bridge(transparent)]
#[derive(bridgerton::serde::Serialize, bridgerton::serde::Deserialize)]
#[serde(crate = "bridgerton::serde")]
#[derive(Clone, Debug, PartialEq)]
pub enum Chain<T> {
    End,
    Link(T, Box<Chain<T>>),
}

#[bridge(transparent)]
#[derive(bridgerton::serde::Serialize, bridgerton::serde::Deserialize)]
#[serde(crate = "bridgerton::serde")]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CollectionTree {
    Leaf(u32),
    Children(Vec<CollectionTree>),
    Map(BTreeMap<String, CollectionTree>),
    Set(BTreeSet<CollectionTree>),
}

#[bridge]
impl Counter {
    pub fn echo_chain(&self, chain: Chain<u32>) -> Chain<u32> {
        chain
    }
    pub fn echo_tree(&self, tree: Tree) -> Tree {
        tree
    }
    pub fn echo_mutual(&self, tree: MutualA) -> MutualA {
        tree
    }
    pub fn echo_collection_tree(&self, tree: CollectionTree) -> CollectionTree {
        tree
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bridgerton::schema::{NativeType, Registry};

    #[test]
    fn only_cases_in_inline_layout_cycles_are_indirect() {
        let mut registry = Registry::default();
        Tree::native_type(&mut registry);
        MutualA::native_type(&mut registry);
        CollectionTree::native_type(&mut registry);
        Chain::<u32>::native_type(&mut registry);
        let swift = registry.swift();
        assert!(!swift.contains("indirect enum"));
        assert_eq!(swift.matches("indirect case").count(), 6);
        for case in ["Next", "Record", "Pair", "Link"] {
            assert!(swift.contains(&format!("indirect case `{case}`")));
        }
        for case in ["Leaf", "Children", "Map", "Set"] {
            assert!(!swift.contains(&format!("indirect case `{case}`")));
        }
    }
}
