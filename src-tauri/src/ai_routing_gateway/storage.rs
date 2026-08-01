use crate::shared_sqlite::{self, SharedSqliteError};
use rusqlite::Connection;

use super::error::{GatewayError, GatewayErrorCategory};

pub(crate) fn open() -> Result<Connection, GatewayError> {
    shared_sqlite::open().map_err(map_storage_error)
}

fn map_storage_error(_error: SharedSqliteError) -> GatewayError {
    GatewayError::new(GatewayErrorCategory::StorageUnavailable, None)
}
