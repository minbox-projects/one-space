//! One-release, best-effort migration of the on-disk state written before the
//! AI Gateway rename.
//!
//! This module is temporary and is deleted in the next version. It is the ONLY
//! place that may name the legacy files and marker; every other module uses the
//! `ai_gateway` names. Removing it must not change behavior once the migration
//! window has passed.

use serde_json::Value;
use std::path::Path;

/// Legacy encrypted config file name, replaced by `super::CONFIG_FILE`.
pub(in crate::ai_gateway) const LEGACY_CONFIG_FILE_NAME: &str = "api_gateway.json";
/// Legacy usage-log database name, replaced by `super::USAGE_DB_FILE`.
pub(in crate::ai_gateway) const LEGACY_USAGE_DB_FILE_NAME: &str = "api_gateway_usage.db";
/// Legacy gateway marker key, replaced by the marker written by
/// `commands::build_gateway_provider`.
pub(in crate::ai_gateway) const LEGACY_GATEWAY_MARKER_KEY: &str = "api_gateway_gateway";

/// Rename one legacy file to its new name when the new file is absent and the
/// legacy file exists, reporting whether a rename actually happened. A
/// pre-existing new file always wins (the legacy file is left untouched), every
/// error is ignored, and the operation is idempotent.
fn rename_if_missing(dir: &Path, legacy: &str, current: &str) -> bool {
    let target = dir.join(current);
    if target.exists() {
        return false;
    }
    let source = dir.join(legacy);
    if source.exists() {
        return std::fs::rename(source, target).is_ok();
    }
    false
}

/// Best-effort in-place migration under `get_app_dir()`. The config file is
/// migrated independently; the usage database's `-wal`/`-shm` sidecars belong
/// to the main database file, so they are migrated ONLY when this call renamed
/// the legacy main database itself. When the legacy main database is not
/// renamed (a current database already exists, or there is no legacy database)
/// every legacy sidecar is left untouched, so a stray sidecar can never be
/// attached to an unrelated current database. Each sidecar is still renamed
/// only when its own target is absent. Never overwrites an existing new file,
/// every error is ignored, and repeated calls are safe.
pub(in crate::ai_gateway) fn migrate_legacy_files() {
    let Ok(dir) = crate::config::get_app_dir() else {
        return;
    };
    rename_if_missing(&dir, LEGACY_CONFIG_FILE_NAME, super::CONFIG_FILE);
    let main_db_renamed =
        rename_if_missing(&dir, LEGACY_USAGE_DB_FILE_NAME, super::USAGE_DB_FILE);
    if main_db_renamed {
        for sidecar in ["-wal", "-shm"] {
            rename_if_missing(
                &dir,
                &format!("{LEGACY_USAGE_DB_FILE_NAME}{sidecar}"),
                &format!("{}{sidecar}", super::USAGE_DB_FILE),
            );
        }
    }
}

/// Whether `provider` still carries the legacy gateway marker at the top level
/// or under `tool_config` (both shapes existed before the rename). It is the
/// compatibility half of `commands::provider_has_gateway_marker`; newly written
/// records always use the current marker.
pub(in crate::ai_gateway) fn has_legacy_gateway_marker(provider: &Value) -> bool {
    provider
        .get(LEGACY_GATEWAY_MARKER_KEY)
        .and_then(Value::as_bool)
        == Some(true)
        || provider
            .get("tool_config")
            .and_then(|tool_config| tool_config.get(LEGACY_GATEWAY_MARKER_KEY))
            .and_then(Value::as_bool)
            == Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;

    struct IsolatedTempHome {
        path: PathBuf,
        _guard: crate::config::test_home::TestHomeGuard,
    }

    impl Drop for IsolatedTempHome {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn isolated_temp_home(name: &str) -> IsolatedTempHome {
        let path = std::env::temp_dir().join(format!(
            "onespace-ai-gateway-migration-{}-{}",
            name,
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&path).expect("create temp home");
        let guard = crate::config::test_home::TestHomeGuard::set(&path);
        IsolatedTempHome {
            path,
            _guard: guard,
        }
    }

    #[test]
    fn migrate_legacy_files_renames_legacy_files_and_sidecars_in_place() {
        let _home = isolated_temp_home("rename");
        let dir = crate::config::get_app_dir().expect("app dir");
        fs::create_dir_all(&dir).expect("create app dir");

        fs::write(dir.join(LEGACY_CONFIG_FILE_NAME), b"legacy-config").expect("write legacy config");
        fs::write(dir.join(LEGACY_USAGE_DB_FILE_NAME), b"legacy-db").expect("write legacy db");
        fs::write(
            dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-wal")),
            b"legacy-wal",
        )
        .expect("write legacy wal");
        fs::write(
            dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-shm")),
            b"legacy-shm",
        )
        .expect("write legacy shm");

        migrate_legacy_files();

        assert_eq!(
            fs::read(dir.join(super::super::CONFIG_FILE)).expect("read config"),
            b"legacy-config",
            "legacy config must be renamed to the current config"
        );
        assert_eq!(
            fs::read(dir.join(super::super::USAGE_DB_FILE)).expect("read db"),
            b"legacy-db",
            "legacy db must be renamed to the current db"
        );
        assert_eq!(
            fs::read(dir.join(format!("{}-wal", super::super::USAGE_DB_FILE))).expect("read wal"),
            b"legacy-wal"
        );
        assert_eq!(
            fs::read(dir.join(format!("{}-shm", super::super::USAGE_DB_FILE))).expect("read shm"),
            b"legacy-shm"
        );
        assert!(!dir.join(LEGACY_CONFIG_FILE_NAME).exists());
        assert!(!dir.join(LEGACY_USAGE_DB_FILE_NAME).exists());
        assert!(!dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-wal")).exists());
        assert!(!dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-shm")).exists());
    }

    #[test]
    fn migrate_legacy_files_never_overwrites_existing_current_files_and_is_idempotent() {
        let _home = isolated_temp_home("no-overwrite");
        let dir = crate::config::get_app_dir().expect("app dir");
        fs::create_dir_all(&dir).expect("create app dir");

        fs::write(dir.join(super::super::CONFIG_FILE), b"current-config")
            .expect("write current config");
        fs::write(dir.join(super::super::USAGE_DB_FILE), b"current-db").expect("write current db");
        fs::write(dir.join(LEGACY_CONFIG_FILE_NAME), b"legacy-config").expect("write legacy config");
        fs::write(dir.join(LEGACY_USAGE_DB_FILE_NAME), b"legacy-db").expect("write legacy db");

        migrate_legacy_files();
        migrate_legacy_files();

        assert_eq!(
            fs::read(dir.join(super::super::CONFIG_FILE)).expect("read config"),
            b"current-config",
            "an existing current config must never be overwritten"
        );
        assert_eq!(
            fs::read(dir.join(super::super::USAGE_DB_FILE)).expect("read db"),
            b"current-db",
            "an existing current db must never be overwritten"
        );
        assert_eq!(
            fs::read(dir.join(LEGACY_CONFIG_FILE_NAME)).expect("read legacy config"),
            b"legacy-config",
            "the legacy config stays untouched while the current file exists"
        );
        assert_eq!(
            fs::read(dir.join(LEGACY_USAGE_DB_FILE_NAME)).expect("read legacy db"),
            b"legacy-db",
            "the legacy db stays untouched while the current file exists"
        );
    }

    /// The usage database and both sidecars migrate together when no current
    /// file exists, so a consistent `db`/`-wal`/`-shm` triple lands in place.
    #[test]
    fn migrate_legacy_files_renames_db_and_sidecars_when_nothing_current_exists() {
        let _home = isolated_temp_home("db-sidecars-rename");
        let dir = crate::config::get_app_dir().expect("app dir");
        fs::create_dir_all(&dir).expect("create app dir");

        fs::write(dir.join(LEGACY_USAGE_DB_FILE_NAME), b"legacy-db").expect("write legacy db");
        fs::write(
            dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-wal")),
            b"legacy-wal",
        )
        .expect("write legacy wal");
        fs::write(
            dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-shm")),
            b"legacy-shm",
        )
        .expect("write legacy shm");

        migrate_legacy_files();

        assert_eq!(
            fs::read(dir.join(super::super::USAGE_DB_FILE)).expect("read db"),
            b"legacy-db"
        );
        assert_eq!(
            fs::read(dir.join(format!("{}-wal", super::super::USAGE_DB_FILE))).expect("read wal"),
            b"legacy-wal"
        );
        assert_eq!(
            fs::read(dir.join(format!("{}-shm", super::super::USAGE_DB_FILE))).expect("read shm"),
            b"legacy-shm"
        );
        assert!(!dir.join(LEGACY_USAGE_DB_FILE_NAME).exists());
        assert!(!dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-wal")).exists());
        assert!(!dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-shm")).exists());
    }

    /// A current main database that already exists blocks the whole database
    /// migration: the legacy main database AND its sidecars stay untouched, and
    /// no stray sidecar is attached to the current database.
    #[test]
    fn migrate_legacy_files_leaves_db_and_sidecars_untouched_when_current_db_exists() {
        let _home = isolated_temp_home("current-db-blocks-sidecars");
        let dir = crate::config::get_app_dir().expect("app dir");
        fs::create_dir_all(&dir).expect("create app dir");

        fs::write(dir.join(super::super::USAGE_DB_FILE), b"current-db").expect("write current db");
        fs::write(dir.join(LEGACY_USAGE_DB_FILE_NAME), b"legacy-db").expect("write legacy db");
        fs::write(
            dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-wal")),
            b"legacy-wal",
        )
        .expect("write legacy wal");
        fs::write(
            dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-shm")),
            b"legacy-shm",
        )
        .expect("write legacy shm");

        migrate_legacy_files();

        assert_eq!(
            fs::read(dir.join(super::super::USAGE_DB_FILE)).expect("read db"),
            b"current-db",
            "an existing current db must never be overwritten"
        );
        assert_eq!(
            fs::read(dir.join(LEGACY_USAGE_DB_FILE_NAME)).expect("read legacy db"),
            b"legacy-db",
            "the legacy db stays untouched while the current db exists"
        );
        assert_eq!(
            fs::read(dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-wal"))).expect("read legacy wal"),
            b"legacy-wal",
            "the legacy wal stays untouched while the current db exists"
        );
        assert_eq!(
            fs::read(dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-shm"))).expect("read legacy shm"),
            b"legacy-shm",
            "the legacy shm stays untouched while the current db exists"
        );
        assert!(
            !dir.join(format!("{}-wal", super::super::USAGE_DB_FILE)).exists(),
            "a stray legacy wal must never be attached to the current db"
        );
        assert!(
            !dir.join(format!("{}-shm", super::super::USAGE_DB_FILE)).exists(),
            "a stray legacy shm must never be attached to the current db"
        );
    }

    /// Orphan sidecars without their legacy main database are never renamed: a
    /// sidecar belongs to the database file, not the other way around.
    #[test]
    fn migrate_legacy_files_does_not_rename_orphan_sidecars_without_legacy_db() {
        let _home = isolated_temp_home("orphan-sidecars");
        let dir = crate::config::get_app_dir().expect("app dir");
        fs::create_dir_all(&dir).expect("create app dir");

        fs::write(
            dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-wal")),
            b"legacy-wal",
        )
        .expect("write legacy wal");
        fs::write(
            dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-shm")),
            b"legacy-shm",
        )
        .expect("write legacy shm");

        migrate_legacy_files();

        assert_eq!(
            fs::read(dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-wal"))).expect("read legacy wal"),
            b"legacy-wal",
            "an orphan legacy wal must stay untouched"
        );
        assert_eq!(
            fs::read(dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-shm"))).expect("read legacy shm"),
            b"legacy-shm",
            "an orphan legacy shm must stay untouched"
        );
        assert!(!dir.join(format!("{}-wal", super::super::USAGE_DB_FILE)).exists());
        assert!(!dir.join(format!("{}-shm", super::super::USAGE_DB_FILE)).exists());
        assert!(!dir.join(super::super::USAGE_DB_FILE).exists());
        assert!(!dir.join(LEGACY_USAGE_DB_FILE_NAME).exists());
    }

    /// When the main database is renamed but one current sidecar already exists,
    /// only the free sidecar moves: the current sidecar keeps its content and
    /// the legacy counterpart is retained rather than overwriting it.
    #[test]
    fn migrate_legacy_files_keeps_current_sidecar_when_both_sidecars_exist() {
        let _home = isolated_temp_home("conflicting-sidecars");
        let dir = crate::config::get_app_dir().expect("app dir");
        fs::create_dir_all(&dir).expect("create app dir");

        fs::write(dir.join(LEGACY_USAGE_DB_FILE_NAME), b"legacy-db").expect("write legacy db");
        fs::write(
            dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-wal")),
            b"legacy-wal",
        )
        .expect("write legacy wal");
        fs::write(
            dir.join(format!("{}-wal", super::super::USAGE_DB_FILE)),
            b"current-wal",
        )
        .expect("write current wal");

        migrate_legacy_files();

        assert_eq!(
            fs::read(dir.join(super::super::USAGE_DB_FILE)).expect("read db"),
            b"legacy-db",
            "the main db is renamed because no current db exists"
        );
        assert_eq!(
            fs::read(dir.join(format!("{}-wal", super::super::USAGE_DB_FILE))).expect("read wal"),
            b"current-wal",
            "the existing current wal must never be overwritten"
        );
        assert_eq!(
            fs::read(dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-wal"))).expect("read legacy wal"),
            b"legacy-wal",
            "the legacy wal is retained when its current target already exists"
        );
        assert!(
            !dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-shm")).exists(),
            "a legacy shm that never existed must not be created"
        );
        assert!(
            !dir.join(format!("{}-shm", super::super::USAGE_DB_FILE)).exists(),
            "a current shm must not be created for a wal-only fixture"
        );
    }

    #[test]
    fn has_legacy_gateway_marker_recognizes_legacy_shapes_only() {
        let legacy_top = json!({ LEGACY_GATEWAY_MARKER_KEY: true });
        assert!(has_legacy_gateway_marker(&legacy_top));
        let legacy_tool_config = json!({ "tool_config": { LEGACY_GATEWAY_MARKER_KEY: true } });
        assert!(has_legacy_gateway_marker(&legacy_tool_config));

        assert!(!has_legacy_gateway_marker(&json!({ "ai_gateway_gateway": true })));
        assert!(!has_legacy_gateway_marker(&json!({ LEGACY_GATEWAY_MARKER_KEY: false })));
        assert!(!has_legacy_gateway_marker(&json!({
            "tool_config": { "npm": "@ai-sdk/openai-compatible" }
        })));
    }
}
