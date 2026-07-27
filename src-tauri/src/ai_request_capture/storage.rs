use super::{
    AiRequestCaptureDetail, AiRequestCaptureHeader, AiRequestCaptureListItem,
    AiRequestCaptureListResult, CaptureFinish, CaptureListQuery, CaptureRecoveryResult,
    CaptureStart, CaptureState, CapturedBody, CAPTURE_BODY_LIMIT_BYTES, CAPTURE_RETENTION_SECONDS,
};
use rusqlite::{params, params_from_iter, types::Value, Connection, Row};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone, Debug)]
pub(crate) struct CaptureStore {
    path: PathBuf,
}

impl CaptureStore {
    pub(crate) fn open(path: PathBuf) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let store = Self { path };
        let mut connection = store.connection()?;
        store.migrate(&mut connection)?;
        Ok(store)
    }

    pub(crate) fn user_version(&self) -> Result<i64, String> {
        self.connection()?
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(|error| error.to_string())
    }

    pub(crate) fn has_index(&self, name: &str) -> Result<bool, String> {
        self.connection()?
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = ?1)",
                [name],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())
    }

    pub(crate) fn begin(&self, start: CaptureStart) -> Result<(), String> {
        let headers =
            serde_json::to_string(&start.request_headers).map_err(|error| error.to_string())?;
        let body = normalize_body(start.request_body);
        self.connection()?.execute(
            "INSERT INTO captures (id, started_at, state, http_version, method, request_path_and_query, upstream_url, request_headers_json, request_body, request_captured_bytes, request_total_bytes, request_truncated, provider, model)
             VALUES (?1, ?2, 'in_progress', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![start.id, start.started_at, start.http_version, start.method, start.request_path_and_query, start.upstream_url, headers, body.data, body.captured_bytes as i64, body.total_bytes as i64, i64::from(body.truncated), start.provider, start.model],
        ).map(|_| ()).map_err(|error| error.to_string())
    }

    pub(crate) fn finish(&self, id: &str, finish: CaptureFinish) -> Result<(), String> {
        let headers =
            serde_json::to_string(&finish.response_headers).map_err(|error| error.to_string())?;
        let body = normalize_body(finish.response_body);
        let connection = self.connection()?;
        let (upstream_url, request_body, existing_provider, existing_model): (
            String,
            Vec<u8>,
            Option<String>,
            Option<String>,
        ) = connection
            .query_row(
                "SELECT upstream_url, request_body, provider, model FROM captures WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .map_err(|error| error.to_string())?;
        let mut enrichment = super::enrichment::enrich(&upstream_url, &request_body, &body.data);
        enrichment.provider = enrichment.provider.or(existing_provider);
        enrichment.model = enrichment.model.or(existing_model);
        let changed = connection.execute(
            "UPDATE captures SET completed_at = ?2, state = ?3, response_status = ?4, response_headers_json = ?5, response_body = ?6, response_captured_bytes = ?7, response_total_bytes = ?8, response_truncated = ?9, duration_ms = MAX(?2 - started_at, 0), error = ?10, provider = ?11, model = ?12, input_tokens = ?13, output_tokens = ?14, total_tokens = ?15 WHERE id = ?1",
            params![id, finish.completed_at, finish.state.as_str(), finish.response_status.map(i64::from), headers, body.data, body.captured_bytes as i64, body.total_bytes as i64, i64::from(body.truncated), finish.error, enrichment.provider, enrichment.model, enrichment.input_tokens.map(|value| value as i64), enrichment.output_tokens.map(|value| value as i64), enrichment.total_tokens.map(|value| value as i64)],
        ).map_err(|error| error.to_string())?;
        if changed == 0 {
            return Err(format!("capture not found: {id}"));
        }
        Ok(())
    }

    pub(crate) fn update_request_body(
        &self,
        id: &str,
        request_body: CapturedBody,
    ) -> Result<(), String> {
        let body = normalize_body(request_body);
        let changed = self
            .connection()?
            .execute(
                "UPDATE captures SET request_body = ?2, request_captured_bytes = ?3, request_total_bytes = ?4, request_truncated = ?5 WHERE id = ?1",
                params![id, body.data, body.captured_bytes as i64, body.total_bytes as i64, i64::from(body.truncated)],
            )
            .map_err(|error| error.to_string())?;
        if changed == 0 {
            return Err(format!("capture not found: {id}"));
        }
        Ok(())
    }

    pub(crate) fn get(&self, id: &str) -> Result<Option<AiRequestCaptureDetail>, String> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, started_at, completed_at, state, http_version, method, request_path_and_query, upstream_url, request_headers_json, request_body, request_captured_bytes, request_total_bytes, request_truncated, response_status, response_headers_json, response_body, response_captured_bytes, response_total_bytes, response_truncated, duration_ms, error, provider, model, input_tokens, output_tokens, total_tokens FROM captures WHERE id = ?1",
        ).map_err(|error| error.to_string())?;
        let mut rows = statement.query([id]).map_err(|error| error.to_string())?;
        rows.next()
            .map_err(|error| error.to_string())?
            .map(read_detail)
            .transpose()
            .map_err(|error| error.to_string())
    }

    pub(crate) fn list(
        &self,
        query: CaptureListQuery,
    ) -> Result<AiRequestCaptureListResult, String> {
        let page = query.page.max(1);
        let page_size = query.page_size.clamp(1, 100);
        let mut values = Vec::new();
        let filter = filter_clause(&query, &mut values);
        let connection = self.connection()?;
        let total = connection
            .query_row(
                &format!("SELECT COUNT(*) FROM captures{filter}"),
                params_from_iter(values.iter()),
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| error.to_string())? as u64;
        values.push(Value::Integer(i64::from(page_size)));
        values.push(Value::Integer(i64::from((page - 1) * page_size)));
        let mut statement = connection.prepare(&format!(
            "SELECT id, started_at, completed_at, state, method, request_path_and_query, upstream_url, response_status, duration_ms, provider, model, input_tokens, output_tokens, total_tokens FROM captures{filter} ORDER BY started_at DESC, id DESC LIMIT ? OFFSET ?",
        )).map_err(|error| error.to_string())?;
        let items = statement
            .query_map(params_from_iter(values.iter()), read_list_item)
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        Ok(AiRequestCaptureListResult {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(crate) fn finished_for_export(
        &self,
        query: CaptureListQuery,
    ) -> Result<Vec<AiRequestCaptureDetail>, String> {
        let mut values = Vec::new();
        let filter = filter_clause(&query, &mut values);
        let filter = if filter.is_empty() {
            " WHERE state <> 'in_progress'".to_string()
        } else {
            format!("{filter} AND state <> 'in_progress'")
        };
        let connection = self.connection()?;
        let mut statement = connection.prepare(&format!(
            "SELECT id, started_at, completed_at, state, http_version, method, request_path_and_query, upstream_url, request_headers_json, request_body, request_captured_bytes, request_total_bytes, request_truncated, response_status, response_headers_json, response_body, response_captured_bytes, response_total_bytes, response_truncated, duration_ms, error, provider, model, input_tokens, output_tokens, total_tokens FROM captures{filter} ORDER BY started_at DESC, id DESC",
        )).map_err(|error| error.to_string())?;
        let records = statement
            .query_map(params_from_iter(values.iter()), read_detail)
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        Ok(records)
    }

    pub(crate) fn clear(&self) -> Result<u64, String> {
        self.connection()?
            .execute("DELETE FROM captures WHERE state <> 'in_progress'", [])
            .map(|count| count as u64)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn recover_interrupted_and_cleanup(
        &self,
        now_millis: i64,
    ) -> Result<CaptureRecoveryResult, String> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let interrupted = transaction.execute(
            "UPDATE captures SET state = 'interrupted', completed_at = ?1, duration_ms = MAX(?1 - started_at, 0), error = COALESCE(error, 'interrupted during previous shutdown') WHERE state = 'in_progress'", [now_millis],
        ).map_err(|error| error.to_string())? as u64;
        let deleted = transaction
            .execute(
                "DELETE FROM captures WHERE started_at < ?1",
                [now_millis.saturating_sub(CAPTURE_RETENTION_SECONDS * 1_000)],
            )
            .map_err(|error| error.to_string())? as u64;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(CaptureRecoveryResult {
            interrupted,
            deleted,
        })
    }

    fn connection(&self) -> Result<Connection, String> {
        let connection = Connection::open(&self.path).map_err(|error| error.to_string())?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(|error| error.to_string())?;
        connection
            .execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")
            .map_err(|error| error.to_string())?;
        Ok(connection)
    }

    fn migrate(&self, connection: &mut Connection) -> Result<(), String> {
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(|error| error.to_string())?;
        if version > 2 {
            return Err(format!("unsupported capture schema version: {version}"));
        }
        if version == 2 {
            return Ok(());
        }
        if version == 1 {
            connection
                .execute_batch(
                    "ALTER TABLE captures ADD COLUMN input_tokens INTEGER;
                     ALTER TABLE captures ADD COLUMN output_tokens INTEGER;
                     ALTER TABLE captures ADD COLUMN total_tokens INTEGER;
                     PRAGMA user_version = 2;",
                )
                .map_err(|error| error.to_string())?;
            return Ok(());
        }
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        transaction.execute_batch(
            "CREATE TABLE captures (
                id TEXT PRIMARY KEY NOT NULL, started_at INTEGER NOT NULL, completed_at INTEGER, state TEXT NOT NULL,
                http_version TEXT NOT NULL, method TEXT NOT NULL, request_path_and_query TEXT NOT NULL, upstream_url TEXT NOT NULL,
                request_headers_json TEXT NOT NULL, request_body BLOB NOT NULL, request_captured_bytes INTEGER NOT NULL,
                request_total_bytes INTEGER NOT NULL, request_truncated INTEGER NOT NULL, response_status INTEGER,
                response_headers_json TEXT, response_body BLOB, response_captured_bytes INTEGER, response_total_bytes INTEGER,
                response_truncated INTEGER, duration_ms INTEGER, error TEXT, provider TEXT, model TEXT,
                input_tokens INTEGER, output_tokens INTEGER, total_tokens INTEGER
            );
            CREATE INDEX captures_started_at_id_idx ON captures(started_at DESC, id DESC);
            CREATE INDEX captures_state_idx ON captures(state);
            CREATE INDEX captures_method_idx ON captures(method);
            CREATE INDEX captures_response_status_idx ON captures(response_status);
            CREATE INDEX captures_provider_idx ON captures(provider);
            CREATE INDEX captures_model_idx ON captures(model);
             PRAGMA user_version = 2;",
        ).map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())
    }
}

fn filter_clause(query: &CaptureListQuery, values: &mut Vec<Value>) -> String {
    let mut conditions = Vec::new();
    if let Some(method) = query
        .method
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        conditions.push("method = ?".to_string());
        values.push(Value::Text(method.to_string()));
    }
    if !query.states.is_empty() {
        conditions.push(format!(
            "state IN ({})",
            std::iter::repeat("?")
                .take(query.states.len())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        values.extend(query.states.iter().map(|state| Value::Text(state.as_str())));
    }
    if let Some(provider) = query
        .provider
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        conditions.push("provider = ?".to_string());
        values.push(Value::Text(provider.to_string()));
    }
    if let Some(model) = query
        .model
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        conditions.push("model = ?".to_string());
        values.push(Value::Text(model.to_string()));
    }
    if let Some(search) = query
        .search
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        conditions.push(
            "(id LIKE ? OR request_path_and_query LIKE ? OR upstream_url LIKE ? OR CAST(request_body AS TEXT) LIKE ? OR CAST(response_body AS TEXT) LIKE ?)".to_string(),
        );
        let pattern = format!("%{search}%");
        values.extend([
            Value::Text(pattern.clone()),
            Value::Text(pattern.clone()),
            Value::Text(pattern.clone()),
            Value::Text(pattern.clone()),
            Value::Text(pattern),
        ]);
    }
    if conditions.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conditions.join(" AND "))
    }
}

fn normalize_body(mut body: CapturedBody) -> CapturedBody {
    body.total_bytes = body.total_bytes.max(body.data.len() as u64);
    if body.data.len() > CAPTURE_BODY_LIMIT_BYTES {
        body.data.truncate(CAPTURE_BODY_LIMIT_BYTES);
        body.truncated = true;
    }
    body.captured_bytes = body.data.len() as u64;
    body.truncated |= body.total_bytes > body.captured_bytes;
    body
}

fn read_list_item(row: &Row<'_>) -> rusqlite::Result<AiRequestCaptureListItem> {
    Ok(AiRequestCaptureListItem {
        id: row.get(0)?,
        started_at: row.get(1)?,
        completed_at: row.get(2)?,
        state: read_state(row.get(3)?)?,
        method: row.get(4)?,
        request_path_and_query: row.get(5)?,
        upstream_url: row.get(6)?,
        response_status: row.get::<_, Option<i64>>(7)?.map(|value| value as u16),
        duration_ms: row.get(8)?,
        provider: row.get(9)?,
        model: row.get(10)?,
        input_tokens: row.get::<_, Option<i64>>(11)?.map(|value| value as u64),
        output_tokens: row.get::<_, Option<i64>>(12)?.map(|value| value as u64),
        total_tokens: row.get::<_, Option<i64>>(13)?.map(|value| value as u64),
        request_body: None,
    })
}

fn read_detail(row: &Row<'_>) -> rusqlite::Result<AiRequestCaptureDetail> {
    let request_headers = read_headers(row.get(8)?)?;
    let response_headers = row
        .get::<_, Option<String>>(14)?
        .map(read_headers)
        .transpose()?
        .unwrap_or_default();
    let request_body = read_body(
        row.get(9)?,
        row.get::<_, i64>(10)? as u64,
        row.get::<_, i64>(11)? as u64,
        row.get::<_, i64>(12)? != 0,
        &request_headers,
    );
    let response_body = read_body(
        row.get::<_, Option<Vec<u8>>>(15)?.unwrap_or_default(),
        row.get::<_, Option<i64>>(16)?.unwrap_or_default() as u64,
        row.get::<_, Option<i64>>(17)?.unwrap_or_default() as u64,
        row.get::<_, Option<i64>>(18)?.unwrap_or_default() != 0,
        &response_headers,
    );
    Ok(AiRequestCaptureDetail {
        id: row.get(0)?,
        started_at: row.get(1)?,
        completed_at: row.get(2)?,
        state: read_state(row.get(3)?)?,
        http_version: row.get(4)?,
        method: row.get(5)?,
        request_path_and_query: row.get(6)?,
        upstream_url: row.get(7)?,
        request_headers,
        request_body,
        response_status: row.get::<_, Option<i64>>(13)?.map(|value| value as u16),
        response_headers,
        response_body,
        duration_ms: row.get(19)?,
        error: row.get(20)?,
        provider: row.get(21)?,
        model: row.get(22)?,
        input_tokens: row.get::<_, Option<i64>>(23)?.map(|value| value as u64),
        output_tokens: row.get::<_, Option<i64>>(24)?.map(|value| value as u64),
        total_tokens: row.get::<_, Option<i64>>(25)?.map(|value| value as u64),
    })
}

fn read_body(
    data: Vec<u8>,
    captured_bytes: u64,
    total_bytes: u64,
    truncated: bool,
    headers: &[AiRequestCaptureHeader],
) -> CapturedBody {
    let mut body = CapturedBody {
        data,
        display: String::new(),
        encoding: None,
        captured_bytes,
        total_bytes,
        truncated,
    };
    let representation = super::enrichment::body_representation(headers, &body);
    body.display = representation.data;
    body.encoding = representation.encoding;
    body
}

fn read_headers(headers: String) -> rusqlite::Result<Vec<AiRequestCaptureHeader>> {
    serde_json::from_str(&headers).map_err(|error| conversion_error(error.to_string()))
}
fn read_state(value: String) -> rusqlite::Result<CaptureState> {
    CaptureState::parse(&value).map_err(conversion_error)
}
fn conversion_error(message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    )
}
