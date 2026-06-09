use crate::config::{self, SkillSourceConfig, StorageConfig};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock, TryLockError};
use std::time::{SystemTime, UNIX_EPOCH};

include!("skills/types.rs");
include!("skills/paths_state.rs");
include!("skills/repository.rs");
include!("skills/catalog_parse.rs");
include!("skills/diff.rs");
include!("skills/sync_apply.rs");
include!("skills/installed_scan.rs");
include!("skills/commands.rs");
include!("skills/tests.rs");
