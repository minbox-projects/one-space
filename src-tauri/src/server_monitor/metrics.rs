//! Pure parsing and calculation for framed Linux collector output.
//!
//! All rate values derive from the two samples inside one collector execution.
//! A value that cannot be derived honestly is returned as unavailable instead
//! of being approximated from another field or an earlier collection.

use crate::server_monitor::types::{
    CollectionFailure, CollectionFailureKind, CollectionOutcome, CollectorOutput, CpuMetric,
    DiskMetric, MemoryMetric, MetricSnapshot, NetworkMetric,
};
use std::collections::BTreeMap;

const SECTION_MARKER_PREFIX: &str = "@@ONESPACE:";
const EXCLUDED_FILESYSTEMS: &[&str] = &[
    "proc", "sysfs", "tmpfs", "devtmpfs", "overlay", "squashfs",
];

/// Splits framed collector output into named sections.
pub(crate) fn parse_sections(raw: &str) -> BTreeMap<String, String> {
    let mut sections: BTreeMap<String, String> = BTreeMap::new();
    let mut current: Option<String> = None;

    for line in raw.lines() {
        if let Some(name) = line
            .strip_prefix(SECTION_MARKER_PREFIX)
            .and_then(|rest| rest.strip_suffix("@@"))
        {
            sections.entry(name.to_string()).or_default();
            current = Some(name.to_string());
        } else if let Some(active) = current.as_ref() {
            let body = sections.entry(active.clone()).or_default();
            body.push_str(line);
            body.push('\n');
        }
    }

    sections
}

/// Builds one self-contained collection result from the script's framed output.
pub(crate) fn build_collection(
    output: &CollectorOutput,
    selected_interface: Option<&str>,
    collected_at_ms: i64,
) -> CollectionOutcome {
    let sections = parse_sections(&output.stdout);

    if let Some(platform) = sections.get("platform") {
        let platform = platform.trim();
        if !platform.is_empty() && !platform.eq_ignore_ascii_case("Linux") {
            return CollectionOutcome::Failed(CollectionFailure {
                kind: CollectionFailureKind::NonLinux,
                message: format!("Server monitoring requires Linux; host reported {platform}"),
            });
        }
    }

    if output.exit_code != Some(0) {
        return CollectionOutcome::Failed(CollectionFailure {
            kind: CollectionFailureKind::Script,
            message: match output.exit_code {
                Some(code) => format!("Collector exited with status {code}"),
                None => "Collector did not report an exit status".to_string(),
            },
        });
    }

    let cpu = calculate_cpu(&sections);
    let memory = calculate_memory(&sections);
    let disks = calculate_disks(&sections);
    let network = calculate_network(&sections, selected_interface);

    let available = usize::from(cpu.is_some())
        + usize::from(memory.is_some())
        + usize::from(disks.is_some())
        + usize::from(network.is_some());

    if available == 0 {
        return CollectionOutcome::Failed(CollectionFailure {
            kind: CollectionFailureKind::NoUsableResources,
            message: "No resource could be collected from this host".to_string(),
        });
    }

    let snapshot = MetricSnapshot {
        collected_at_ms,
        cpu,
        memory,
        disks,
        network,
    };

    if available == 4 {
        CollectionOutcome::Success(snapshot)
    } else {
        CollectionOutcome::Partial(snapshot)
    }
}

fn calculate_cpu(sections: &BTreeMap<String, String>) -> Option<CpuMetric> {
    let first = sections.get("stat_1")?;
    let second = sections.get("stat_2")?;

    let first_line_count = first.lines().filter(|line| !line.trim().is_empty()).count();
    let second_line_count = second.lines().filter(|line| !line.trim().is_empty()).count();
    if first_line_count == 0 || first_line_count != second_line_count {
        return None;
    }

    let before = parse_cpu_line(first)?;
    let after = parse_cpu_line(second)?;

    let mut total_before = 0u64;
    let mut total_after = 0u64;
    let mut busy_delta = 0u64;
    for index in 0..before.len() {
        if after[index] < before[index] {
            return None;
        }
        let delta = after[index] - before[index];
        total_before = total_before.checked_add(before[index])?;
        total_after = total_after.checked_add(after[index])?;
        // Fields are user, nice, system, idle, iowait, irq, softirq and steal;
        // idle and iowait stay out of the busy total.
        if index != 3 && index != 4 {
            busy_delta = busy_delta.checked_add(delta)?;
        }
    }

    let total_delta = total_after.checked_sub(total_before)?;
    if total_delta == 0 {
        return None;
    }

    Some(CpuMetric {
        usage_percent: 100.0 * busy_delta as f64 / total_delta as f64,
    })
}

fn parse_cpu_line(section: &str) -> Option<Vec<u64>> {
    let line = section.lines().find(|line| {
        line.trim_start()
            .strip_prefix("cpu")
            .map(|rest| rest.chars().next().map(char::is_whitespace).unwrap_or(false))
            .unwrap_or(false)
    })?;
    let values = line
        .split_whitespace()
        .skip(1)
        .map(|value| value.parse::<u64>().ok())
        .collect::<Option<Vec<_>>>()?;
    if values.len() < 8 {
        return None;
    }
    Some(values[..8].to_vec())
}

fn calculate_memory(sections: &BTreeMap<String, String>) -> Option<MemoryMetric> {
    let meminfo = sections.get("meminfo")?;
    let total_kb = parse_meminfo_value(meminfo, "MemTotal:")?;
    let available_kb = parse_meminfo_value(meminfo, "MemAvailable:")?;
    if total_kb == 0 || available_kb > total_kb {
        return None;
    }
    let used_kb = total_kb - available_kb;
    Some(MemoryMetric {
        total_bytes: total_kb.saturating_mul(1024),
        used_bytes: used_kb.saturating_mul(1024),
        available_bytes: available_kb.saturating_mul(1024),
        used_percent: 100.0 * used_kb as f64 / total_kb as f64,
    })
}

fn parse_meminfo_value(meminfo: &str, key: &str) -> Option<u64> {
    meminfo
        .lines()
        .find_map(|line| line.trim().strip_prefix(key))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|value| value.parse::<u64>().ok())
}

fn calculate_disks(sections: &BTreeMap<String, String>) -> Option<Vec<DiskMetric>> {
    let df = sections.get("df")?;
    let disks = parse_disks(df);
    if disks.is_empty() {
        None
    } else {
        Some(disks)
    }
}

fn parse_disks(df: &str) -> Vec<DiskMetric> {
    let mut lines = df.lines().filter(|line| !line.trim().is_empty());
    let Some(header) = lines.next() else {
        return Vec::new();
    };
    let has_type_column = header.split_whitespace().any(|token| token == "Type");
    let fixed_columns = if has_type_column { 6 } else { 5 };

    let mut disks = Vec::new();
    for line in lines {
        let tokens = line.split_whitespace().collect::<Vec<_>>();
        if tokens.len() < fixed_columns + 1 {
            continue;
        }
        let (filesystem, filesystem_type, size, used, available) = if has_type_column {
            (
                tokens[0],
                Some(tokens[1]),
                tokens[2],
                tokens[3],
                tokens[4],
            )
        } else {
            (tokens[0], None, tokens[1], tokens[2], tokens[3])
        };

        if is_excluded_filesystem(filesystem)
            || filesystem_type
                .map(is_excluded_filesystem)
                .unwrap_or(false)
        {
            continue;
        }

        let Ok(total_kb) = size.parse::<i64>() else {
            continue;
        };
        if total_kb <= 0 {
            continue;
        }
        let Ok(used_kb) = used.parse::<i64>() else {
            continue;
        };
        let Ok(available_kb) = available.parse::<i64>() else {
            continue;
        };
        let used_kb = used_kb.max(0);
        let usable_kb = available_kb.max(0);
        let mount_point = tokens[fixed_columns..].join(" ");
        if mount_point.is_empty() {
            continue;
        }

        disks.push(DiskMetric {
            filesystem: filesystem.to_string(),
            mount_point,
            total_bytes: (total_kb as u64).saturating_mul(1024),
            used_bytes: (used_kb as u64).saturating_mul(1024),
            available_bytes: (usable_kb as u64).saturating_mul(1024),
            used_percent: 100.0 * used_kb as f64 / total_kb as f64,
        });
    }
    disks
}

fn is_excluded_filesystem(value: &str) -> bool {
    EXCLUDED_FILESYSTEMS
        .iter()
        .any(|excluded| value.eq_ignore_ascii_case(excluded))
}

fn calculate_network(
    sections: &BTreeMap<String, String>,
    selected_interface: Option<&str>,
) -> Option<NetworkMetric> {
    let boot_first = sections.get("boot_id_1")?.trim();
    let boot_second = sections.get("boot_id_2")?.trim();
    if boot_first.is_empty() || boot_first != boot_second {
        return None;
    }

    let uptime_first = parse_uptime(sections.get("uptime_1")?)?;
    let uptime_second = parse_uptime(sections.get("uptime_2")?)?;
    let elapsed = uptime_second - uptime_first;
    if !elapsed.is_finite() || elapsed <= 0.0 {
        return None;
    }

    let first = parse_net_dev(sections.get("net_1")?);
    let second = parse_net_dev(sections.get("net_2")?);

    if let Some(requested) = selected_interface {
        let before = first.get(requested)?;
        let after = second.get(requested)?;
        return network_rate(requested, before, after, elapsed);
    }

    for (name, before) in &first {
        let Some(after) = second.get(name) else {
            continue;
        };
        if let Some(metric) = network_rate(name, before, after, elapsed) {
            return Some(metric);
        }
    }
    None
}

fn parse_uptime(section: &str) -> Option<f64> {
    section
        .split_whitespace()
        .next()
        .and_then(|value| value.parse::<f64>().ok())
}

fn parse_net_dev(section: &str) -> BTreeMap<String, (u64, u64)> {
    let mut interfaces = BTreeMap::new();
    for line in section.lines() {
        let Some((name, values)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() || name == "lo" {
            continue;
        }
        let values = values
            .split_whitespace()
            .map(|value| value.parse::<u64>().ok())
            .collect::<Option<Vec<_>>>();
        let Some(values) = values else {
            continue;
        };
        if values.len() < 9 {
            continue;
        }
        interfaces.insert(name.to_string(), (values[0], values[8]));
    }
    interfaces
}

fn network_rate(
    interface: &str,
    before: &(u64, u64),
    after: &(u64, u64),
    elapsed: f64,
) -> Option<NetworkMetric> {
    if after.0 < before.0 || after.1 < before.1 {
        return None;
    }
    let rx_delta = after.0 - before.0;
    let tx_delta = after.1 - before.1;
    Some(NetworkMetric {
        interface: interface.to_string(),
        rx_bytes_per_sec: rx_delta as f64 / elapsed,
        tx_bytes_per_sec: tx_delta as f64 / elapsed,
        rx_delta_bytes: rx_delta,
        tx_delta_bytes: tx_delta,
        elapsed_seconds: elapsed,
    })
}
