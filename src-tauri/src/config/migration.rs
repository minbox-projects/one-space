use super::storage::{copy_tree_if_missing, get_shared_data_dir_for};
use super::types::StorageConfig;

pub(super) fn migrate_shared_storage_if_needed(
    old_cfg: &StorageConfig,
    new_cfg: &StorageConfig,
) -> Result<(), String> {
    let changed = old_cfg.storage_type != new_cfg.storage_type
        || (new_cfg.storage_type == "local"
            && old_cfg.local_storage_path != new_cfg.local_storage_path)
        || (new_cfg.storage_type == "icloud"
            && old_cfg.icloud_storage_path != new_cfg.icloud_storage_path);

    if !changed {
        return Ok(());
    }

    let src = get_shared_data_dir_for(old_cfg)?;
    let dst = get_shared_data_dir_for(new_cfg)?;
    if src != dst && src.exists() {
        copy_tree_if_missing(&src, &dst)?;
    }
    Ok(())
}
