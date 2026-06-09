#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, thread::sleep, time::Duration};

    include!("tests/helpers.rs");
    include!("tests/codex_projection.rs");
    include!("tests/claude_projection.rs");
    include!("tests/launcher_sync.rs");
    include!("tests/session_history.rs");
    include!("tests/favorites_permissions.rs");
    include!("tests/claude_profiles.rs");
    include!("tests/service_provider_history.rs");
    include!("tests/migration_conversion.rs");
    include!("tests/sync_import.rs");
    include!("tests/migration_ids.rs");
}
