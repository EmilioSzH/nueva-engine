//! Schema migration support for Nueva projects.
//!
//! Handles upgrading project files from older schema versions to the current version.
//! Migrations are applied sequentially, allowing projects to be upgraded across
//! multiple version jumps.

use std::collections::HashMap;

use serde_json::Value;

use crate::state::error::{NuevaError, Result};

/// Current schema version for project files.
pub const CURRENT_SCHEMA_VERSION: &str = "1.0.0";

/// Current Nueva application version.
pub const NUEVA_VERSION: &str = "0.1.0";

/// Type alias for migration functions.
/// Takes a JSON value and returns the migrated JSON value or an error.
type MigrationFn = fn(Value) -> Result<Value>;

/// Lazily initialized migration registry.
/// Maps (from_version, to_version) tuples to migration functions.
fn get_migration_registry() -> HashMap<(String, String), MigrationFn> {
    let mut registry: HashMap<(String, String), MigrationFn> = HashMap::new();

    // Register placeholder migrations for future versions.
    // When new schema versions are added, add migration functions here.
    //
    // Example:
    // registry.insert(
    //     ("1.0.0".to_string(), "1.1.0".to_string()),
    //     migrate_1_0_0_to_1_1_0,
    // );

    // Placeholder: Migration from 1.0.0 to 1.1.0 (for future use)
    registry.insert(
        ("1.0.0".to_string(), "1.1.0".to_string()),
        migrate_1_0_0_to_1_1_0,
    );

    // Placeholder: Migration from 1.1.0 to 1.2.0 (for future use)
    registry.insert(
        ("1.1.0".to_string(), "1.2.0".to_string()),
        migrate_1_1_0_to_1_2_0,
    );

    // Placeholder: Migration from 1.2.0 to 2.0.0 (for future use)
    registry.insert(
        ("1.2.0".to_string(), "2.0.0".to_string()),
        migrate_1_2_0_to_2_0_0,
    );

    registry
}

/// Get all known schema versions in order.
fn get_version_order() -> Vec<&'static str> {
    vec!["1.0.0", "1.1.0", "1.2.0", "2.0.0"]
}

/// Migrate a project from its current schema version to the target version.
///
/// # Arguments
/// * `data` - The project JSON data to migrate
///
/// # Returns
/// The migrated JSON data with updated schema_version field.
///
/// # Errors
/// Returns `NuevaError::MigrationError` if migration fails or no migration path exists.
/// Returns `NuevaError::InvalidSchemaVersion` if the schema version is not recognized.
pub fn migrate_project(mut data: Value) -> Result<Value> {
    // Extract current schema version, defaulting to "1.0.0" if missing
    let current_version = data
        .get("schema_version")
        .and_then(|v| v.as_str())
        .unwrap_or("1.0.0")
        .to_string();

    let target_version = CURRENT_SCHEMA_VERSION;

    // If already at target version, ensure schema_version field is set and return
    if current_version == target_version {
        // Ensure schema_version field exists (it may have been defaulted)
        if let Some(obj) = data.as_object_mut() {
            obj.entry("schema_version".to_string())
                .or_insert_with(|| Value::String(target_version.to_string()));
        }
        return Ok(data);
    }

    // Find migration path
    let path = find_migration_path(&current_version, target_version);

    if path.is_empty() && current_version != target_version {
        // Check if the version is newer than what we support
        let known_versions = get_version_order();
        if !known_versions.contains(&current_version.as_str()) {
            return Err(NuevaError::InvalidSchemaVersion {
                version: current_version,
            });
        }

        // Check if we're trying to downgrade (target is older than current)
        let current_idx = known_versions
            .iter()
            .position(|&v| v == current_version)
            .unwrap_or(0);
        let target_idx = known_versions
            .iter()
            .position(|&v| v == target_version)
            .unwrap_or(0);

        if current_idx > target_idx {
            // Project is from a newer version - we can't downgrade
            return Err(NuevaError::MigrationError {
                from: current_version,
                to: target_version.to_string(),
                reason: "Cannot downgrade project from newer schema version".to_string(),
            });
        }

        // No migration path found
        return Err(NuevaError::MigrationError {
            from: current_version,
            to: target_version.to_string(),
            reason: "No migration path found".to_string(),
        });
    }

    // Apply each migration in sequence
    let registry = get_migration_registry();

    for (from, to) in path {
        let migration_fn = registry.get(&(from.clone(), to.clone())).ok_or_else(|| {
            NuevaError::MigrationError {
                from: from.clone(),
                to: to.clone(),
                reason: "Migration function not found in registry".to_string(),
            }
        })?;

        // Apply the migration
        data = migration_fn(data).map_err(|e| NuevaError::MigrationError {
            from: from.clone(),
            to: to.clone(),
            reason: format!("Migration failed: {}", e),
        })?;

        // Update schema_version after successful migration
        if let Some(obj) = data.as_object_mut() {
            obj.insert("schema_version".to_string(), Value::String(to.clone()));
        }
    }

    Ok(data)
}

/// Find the sequence of migrations needed to go from one version to another.
///
/// # Arguments
/// * `from` - The starting schema version
/// * `to` - The target schema version
///
/// # Returns
/// A vector of (from_version, to_version) tuples representing the migration path.
/// Returns an empty vector if from == to or if no path exists.
pub fn find_migration_path(from: &str, to: &str) -> Vec<(String, String)> {
    if from == to {
        return Vec::new();
    }

    let versions = get_version_order();
    let registry = get_migration_registry();

    // Find indices of from and to versions
    let from_idx = match versions.iter().position(|&v| v == from) {
        Some(idx) => idx,
        None => return Vec::new(), // Unknown version
    };

    let to_idx = match versions.iter().position(|&v| v == to) {
        Some(idx) => idx,
        None => return Vec::new(), // Unknown version
    };

    // We only support forward migrations (upgrades)
    if from_idx >= to_idx {
        return Vec::new();
    }

    // Build migration path by checking which migrations exist in the registry
    let mut path = Vec::new();
    let mut current_idx = from_idx;

    while current_idx < to_idx {
        let current = versions[current_idx].to_string();

        // Try to find a migration from current version to any later version
        let mut found_next = false;
        for next_idx in (current_idx + 1)..=to_idx {
            let next = versions[next_idx].to_string();
            if registry.contains_key(&(current.clone(), next.clone())) {
                path.push((current, next));
                current_idx = next_idx;
                found_next = true;
                break;
            }
        }

        if !found_next {
            // No migration found from current version
            return Vec::new();
        }
    }

    path
}

// ============================================================================
// Placeholder Migration Functions
// ============================================================================
// These are example migrations for future schema versions.
// They currently pass through unchanged but demonstrate the pattern.

/// Placeholder migration from 1.0.0 to 1.1.0.
///
/// Future changes might include:
/// - Adding new fields with default values
/// - Restructuring existing data
fn migrate_1_0_0_to_1_1_0(data: Value) -> Result<Value> {
    // Example: Add a new optional field that didn't exist in 1.0.0
    // let mut data = data;
    // if let Some(obj) = data.as_object_mut() {
    //     obj.entry("new_field").or_insert(Value::Null);
    // }

    // For now, pass through unchanged
    Ok(data)
}

/// Placeholder migration from 1.1.0 to 1.2.0.
///
/// Future changes might include:
/// - Renaming fields
/// - Changing field types
fn migrate_1_1_0_to_1_2_0(data: Value) -> Result<Value> {
    // Example: Rename a field
    // let mut data = data;
    // if let Some(obj) = data.as_object_mut() {
    //     if let Some(old_value) = obj.remove("old_field_name") {
    //         obj.insert("new_field_name".to_string(), old_value);
    //     }
    // }

    // For now, pass through unchanged
    Ok(data)
}

/// Placeholder migration from 1.2.0 to 2.0.0.
///
/// Major version changes might include:
/// - Breaking changes to data structure
/// - Removing deprecated fields
/// - Restructuring entire sections
fn migrate_1_2_0_to_2_0_0(data: Value) -> Result<Value> {
    // Example: Major restructuring
    // let mut data = data;
    // if let Some(obj) = data.as_object_mut() {
    //     // Move nested data to new location
    //     if let Some(layer2) = obj.get("layer2").cloned() {
    //         if let Some(chain) = layer2.get("chain") {
    //             obj.insert("effects_chain".to_string(), chain.clone());
    //         }
    //     }
    // }

    // For now, pass through unchanged
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_migrate_current_version_unchanged() {
        let data = json!({
            "schema_version": CURRENT_SCHEMA_VERSION,
            "test_field": "test_value"
        });

        let result = migrate_project(data.clone()).unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn test_migrate_missing_version_defaults_to_1_0_0() {
        let data = json!({
            "test_field": "test_value"
        });

        let result = migrate_project(data).unwrap();
        // Should be unchanged since 1.0.0 is current
        assert_eq!(
            result.get("schema_version").and_then(|v| v.as_str()),
            Some(CURRENT_SCHEMA_VERSION)
        );
    }

    #[test]
    fn test_find_migration_path_same_version() {
        let path = find_migration_path("1.0.0", "1.0.0");
        assert!(path.is_empty());
    }

    #[test]
    fn test_find_migration_path_sequential() {
        let path = find_migration_path("1.0.0", "1.1.0");
        assert_eq!(path.len(), 1);
        assert_eq!(path[0], ("1.0.0".to_string(), "1.1.0".to_string()));
    }

    #[test]
    fn test_find_migration_path_multiple_steps() {
        let path = find_migration_path("1.0.0", "2.0.0");
        assert_eq!(path.len(), 3);
        assert_eq!(path[0], ("1.0.0".to_string(), "1.1.0".to_string()));
        assert_eq!(path[1], ("1.1.0".to_string(), "1.2.0".to_string()));
        assert_eq!(path[2], ("1.2.0".to_string(), "2.0.0".to_string()));
    }

    #[test]
    fn test_find_migration_path_unknown_version() {
        let path = find_migration_path("0.9.0", "1.0.0");
        assert!(path.is_empty());
    }

    #[test]
    fn test_find_migration_path_downgrade_not_supported() {
        let path = find_migration_path("2.0.0", "1.0.0");
        assert!(path.is_empty());
    }

    #[test]
    fn test_migrate_invalid_schema_version() {
        let data = json!({
            "schema_version": "0.5.0",
            "test_field": "test_value"
        });

        let result = migrate_project(data);
        assert!(result.is_err());
        if let Err(NuevaError::InvalidSchemaVersion { version }) = result {
            assert_eq!(version, "0.5.0");
        } else {
            panic!("Expected InvalidSchemaVersion error");
        }
    }

    #[test]
    fn test_constants() {
        assert_eq!(CURRENT_SCHEMA_VERSION, "1.0.0");
        assert_eq!(NUEVA_VERSION, "0.1.0");
    }
}
