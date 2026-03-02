//! Agent metadata domain type.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

/// Agent metadata contract type.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct AgentMetadata(HashMap<String, String>);

impl AgentMetadata {
    pub fn new() -> Self {
        Self(HashMap::new())
    }
}

impl From<HashMap<String, String>> for AgentMetadata {
    fn from(value: HashMap<String, String>) -> Self {
        Self(value)
    }
}

impl From<AgentMetadata> for HashMap<String, String> {
    fn from(value: AgentMetadata) -> Self {
        value.0
    }
}

impl FromIterator<(String, String)> for AgentMetadata {
    fn from_iter<T: IntoIterator<Item = (String, String)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl Deref for AgentMetadata {
    type Target = HashMap<String, String>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for AgentMetadata {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl IntoIterator for AgentMetadata {
    type Item = (String, String);
    type IntoIter = std::collections::hash_map::IntoIter<String, String>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a AgentMetadata {
    type Item = (&'a String, &'a String);
    type IntoIter = std::collections::hash_map::Iter<'a, String, String>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a mut AgentMetadata {
    type Item = (&'a String, &'a mut String);
    type IntoIter = std::collections::hash_map::IterMut<'a, String, String>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}
