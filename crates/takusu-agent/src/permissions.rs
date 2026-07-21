use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// Permissions is serialized as a flat map of `target:operation` -> bool so that
// mobile clients can send it directly without wrapping it in an `allow` field.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Permissions {
    #[serde(default, flatten)]
    pub allow: BTreeMap<String, bool>,
}

impl Permissions {
    /// Returns the explicitly configured value for a `(target, operation)` pair,
    /// checking wildcard patterns from most specific to least specific.
    ///
    /// Lookup order:
    /// 1. `target:operation`
    /// 2. `target:*`
    /// 3. `*:operation`
    /// 4. `*:*`
    pub fn resolve(&self, target: &str, operation: &str) -> Option<bool> {
        let exact = format!("{target}:{operation}");
        if let Some(&allowed) = self.allow.get(&exact) {
            return Some(allowed);
        }
        let target_wildcard = format!("{target}:*");
        if let Some(&allowed) = self.allow.get(&target_wildcard) {
            return Some(allowed);
        }
        let op_wildcard = format!("*:{operation}");
        if let Some(&allowed) = self.allow.get(&op_wildcard) {
            return Some(allowed);
        }
        if let Some(&allowed) = self.allow.get("*:*") {
            return Some(allowed);
        }
        None
    }

    pub fn is_allowed(&self, target: &str, operation: &str) -> bool {
        self.resolve(target, operation).unwrap_or(false)
    }

    pub fn set(&mut self, target: &str, operation: &str, allowed: bool) {
        self.allow.insert(format!("{target}:{operation}"), allowed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_json_deserializes_into_permissions() {
        // Mobile sends permissions as a flat map, not wrapped in `allow`.
        let flat = r#"{"schedule:generate":true}"#;
        let parsed: Permissions = serde_json::from_str(flat).unwrap();
        assert!(parsed.is_allowed("schedule", "generate"));
    }
}
