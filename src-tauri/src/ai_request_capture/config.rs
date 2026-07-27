use super::{AiRequestCaptureConfig, AiRequestCaptureValidationError};
use std::fs;
use std::net::{IpAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use url::{Host, Url};
use uuid::Uuid;

pub(crate) fn capture_data_dir_in(app_dir: &Path) -> PathBuf {
    app_dir.join("data").join("ai-request-capture")
}
pub(crate) fn config_path_in(app_dir: &Path) -> PathBuf {
    capture_data_dir_in(app_dir).join("config.json")
}
pub(crate) fn database_path_in(app_dir: &Path) -> PathBuf {
    capture_data_dir_in(app_dir).join("captures.sqlite3")
}
pub(crate) fn app_dir() -> Result<PathBuf, String> {
    crate::config::get_app_dir()
}

pub(crate) fn read_config() -> Result<AiRequestCaptureConfig, String> {
    read_config_in(&app_dir()?)
}
pub(crate) fn read_config_in(app_dir: &Path) -> Result<AiRequestCaptureConfig, String> {
    let path = config_path_in(app_dir);
    if !path.exists() {
        return Ok(AiRequestCaptureConfig::default());
    }
    serde_json::from_str(&fs::read_to_string(path).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())
}
pub(crate) fn write_config(config: &AiRequestCaptureConfig) -> Result<(), String> {
    write_config_in(&app_dir()?, config)
}
pub(crate) fn write_config_in(
    app_dir: &Path,
    config: &AiRequestCaptureConfig,
) -> Result<(), String> {
    let path = config_path_in(app_dir);
    let parent = path.parent().ok_or("capture config path has no parent")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temporary = parent.join(format!(".config-{}.tmp", Uuid::new_v4()));
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(config).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    fs::File::open(&temporary)
        .and_then(|file| file.sync_all())
        .map_err(|error| error.to_string())?;
    fs::rename(temporary, path).map_err(|error| error.to_string())
}
pub(crate) fn validation_errors(
    config: &AiRequestCaptureConfig,
) -> Vec<AiRequestCaptureValidationError> {
    let mut errors = Vec::new();
    if config.port == 0 {
        errors.push(error("port", "port must be between 1 and 65535"));
    }
    let upstream = config.upstream_base_url.trim();
    if upstream.is_empty() {
        if config.enabled {
            errors.push(error(
                "upstreamBaseUrl",
                "upstream base URL is required when capture is enabled",
            ));
        }
        return errors;
    }
    if upstream
        .split_once("://")
        .map(|(_, authority)| authority.starts_with('/'))
        .unwrap_or(false)
    {
        errors.push(error(
            "upstreamBaseUrl",
            "upstream base URL requires a host",
        ));
        return errors;
    }
    let url = match Url::parse(upstream) {
        Ok(url) => url,
        Err(_) => {
            errors.push(error("upstreamBaseUrl", "upstream base URL is invalid"));
            return errors;
        }
    };
    if !matches!(url.scheme(), "http" | "https") {
        errors.push(error(
            "upstreamBaseUrl",
            "upstream base URL must use HTTP or HTTPS",
        ));
    }
    if url.host().is_none() {
        errors.push(error(
            "upstreamBaseUrl",
            "upstream base URL requires a host",
        ));
    }
    if url.query().is_some() {
        errors.push(error(
            "upstreamBaseUrl",
            "upstream base URL must not include a query string",
        ));
    }
    if url.fragment().is_some() {
        errors.push(error(
            "upstreamBaseUrl",
            "upstream base URL must not include a fragment",
        ));
    }
    if url.host().map(is_loopback_host).unwrap_or(false)
        && url.port_or_known_default() == Some(config.port)
    {
        errors.push(error(
            "upstreamBaseUrl",
            "upstream base URL must not target this loopback listener",
        ));
    }
    errors
}
fn error(field: &str, message: &str) -> AiRequestCaptureValidationError {
    AiRequestCaptureValidationError {
        field: field.to_string(),
        message: message.to_string(),
    }
}
fn is_loopback_host(host: Host<&str>) -> bool {
    match host {
        Host::Ipv4(ip) => ip.is_loopback(),
        Host::Ipv6(ip) => ip.is_loopback(),
        Host::Domain(domain) => {
            domain.eq_ignore_ascii_case("localhost")
                || (domain, 0)
                    .to_socket_addrs()
                    .map(|mut addresses| {
                        addresses.any(|address| match address.ip() {
                            IpAddr::V4(ip) => ip.is_loopback(),
                            IpAddr::V6(ip) => ip.is_loopback(),
                        })
                    })
                    .unwrap_or(false)
        }
    }
}
