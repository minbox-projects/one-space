use crate::server_monitor::metrics::{build_collection, parse_sections};
use crate::server_monitor::types::{
    CollectionFailureKind, CollectionOutcome, CollectorOutput, CpuMetric, DiskMetric, MemoryMetric,
    MetricSnapshot, NetworkMetric,
};
use std::collections::BTreeMap;

fn frame(name: &str, body: &str) -> String {
    format!("@@ONESPACE:{}@@\n{}", name, body)
}

#[test]
fn parse_sections_splits_on_markers() {
    let raw = format!(
        "{}{}\n",
        frame("boot_id_1", "abc\n"),
        frame("uptime_1", "10.0\n")
    );
    let sections = parse_sections(&raw);
    assert_eq!(sections.get("boot_id_1").map(|s| s.trim()), Some("abc"));
    assert_eq!(sections.get("uptime_1").map(|s| s.trim()), Some("10.0"));
}

#[test]
fn cpu_busy_percent_excludes_guest_and_counts_iowait_idle() {
    // stat_1 aggregate cpu: user=100 nice=0 system=50 idle=800 iowait=50 irq=0 softirq=0 steal=0 guest=900 guest_nice=50
    // stat_2 adds user+50, idle+100, guest+100
    let stat1 = "cpu  100 0 50 800 50 0 0 0 900 50\ncpu0 1 2 3 4 5 6 7 8 9 10\n";
    let stat2 = "cpu  150 0 50 900 50 0 0 0 1000 50\ncpu0 1 2 3 4 5 6 7 8 9 10\n";
    let raw = format!(
        "{}{}{}{}{}{}{}{}{}{}",
        frame("boot_id_1", "b1\n"),
        frame("boot_id_2", "b1\n"),
        frame("uptime_1", "10.0\n"),
        frame("uptime_2", "11.0\n"),
        frame("stat_1", stat1),
        frame("stat_2", stat2),
        frame("net_1", "eth0: 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0\n"),
        frame("net_2", "eth0: 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0\n"),
        frame("meminfo", "MemTotal: 1000000 kB\nMemAvailable: 400000 kB\n"),
        frame("df", "Filesystem 1K-blocks Used Available Use% Mounted on\n/dev/sda1 1000 500 500 50% /\n"),
    );
    let output = CollectorOutput {
        stdout: raw,
        exit_code: Some(0),
    };
    let outcome = build_collection(&output, None, 123);
    match outcome {
        CollectionOutcome::Success(s) | CollectionOutcome::Partial(s) => {
            let cpu = s.cpu.expect("cpu");
            // total delta = (150-100)+(0)+(50-50)+(900-800)+(50-50)+... = 50+0+0+100+0 = 150? 
            // busy = total - idle - iowait = 150 - 100 - 0 = 50; percent = 50/150*100 = 33.333...
            // idle delta 100, iowait delta 0
            let expected = 50.0 / 150.0 * 100.0;
            assert!((cpu.usage_percent - expected).abs() < 1e-9, "{}", cpu.usage_percent);
        }
        CollectionOutcome::Failed(f) => panic!("failed: {:?}", f),
    }
}

#[test]
fn memory_conversion_and_missing_memavailable() {
    let raw = format!(
        "{}{}{}{}{}{}{}{}{}{}",
        frame("boot_id_1", "b1\n"),
        frame("boot_id_2", "b1\n"),
        frame("uptime_1", "10.0\n"),
        frame("uptime_2", "11.0\n"),
        frame("stat_1", "cpu  1 0 1 1 0 0 0 0 0 0\n"),
        frame("stat_2", "cpu  2 0 1 1 0 0 0 0 0 0\n"),
        frame("net_1", "eth0: 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0\n"),
        frame("net_2", "eth0: 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0\n"),
        frame("meminfo", "MemTotal: 1000000 kB\nMemAvailable: 400000 kB\n"),
        frame("df", "Filesystem 1K-blocks Used Available Use% Mounted on\n/dev/sda1 1000 500 500 50% /\n"),
    );
    let output = CollectorOutput { stdout: raw.clone(), exit_code: Some(0) };
    let outcome = build_collection(&output, None, 1);
    match outcome {
        CollectionOutcome::Success(s) | CollectionOutcome::Partial(s) => {
            let m = s.memory.expect("memory");
            assert_eq!(m.total_bytes, 1_000_000 * 1024);
            assert_eq!(m.available_bytes, 400_000 * 1024);
            assert_eq!(m.used_bytes, 600_000 * 1024);
            assert!((m.used_percent - 60.0).abs() < 1e-9);
        }
        CollectionOutcome::Failed(f) => panic!("failed: {:?}", f),
    }

    // Missing MemAvailable -> memory unavailable
    let raw2 = raw.replace("MemAvailable: 400000 kB\n", "");
    let output2 = CollectorOutput { stdout: raw2, exit_code: Some(0) };
    let outcome2 = build_collection(&output2, None, 1);
    match outcome2 {
        CollectionOutcome::Success(s) | CollectionOutcome::Partial(s) => {
            assert!(s.memory.is_none());
        }
        CollectionOutcome::Failed(_) => {}
    }
}

#[test]
fn disk_parsing_excludes_pseudofs_and_handles_spaces() {
    let df = "Filesystem 1K-blocks Used Available Use% Mounted on\n\
tmpfs 100 10 90 10% /run\n\
overlay 200 100 100 50% /var/lib/docker\n\
squashfs 50 50 0 100% /snap/x\n\
/dev/sda1 1024 512 512 50% /\n\
/dev/sdb1 2048 1800 248 88% /mnt/data disk\n";
    let raw = format!(
        "{}{}{}{}{}{}{}{}{}{}",
        frame("boot_id_1", "b1\n"),
        frame("boot_id_2", "b1\n"),
        frame("uptime_1", "10.0\n"),
        frame("uptime_2", "11.0\n"),
        frame("stat_1", "cpu  1 0 1 1 0 0 0 0 0 0\n"),
        frame("stat_2", "cpu  2 0 1 1 0 0 0 0 0 0\n"),
        frame("net_1", "eth0: 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0\n"),
        frame("net_2", "eth0: 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0\n"),
        frame("meminfo", "MemTotal: 1000000 kB\nMemAvailable: 400000 kB\n"),
        frame("df", df),
    );
    let output = CollectorOutput { stdout: raw.clone(), exit_code: Some(0) };
    let outcome = build_collection(&output, None, 1);
    match outcome {
        CollectionOutcome::Success(s) | CollectionOutcome::Partial(s) => {
            let disks = s.disks.expect("disks");
            // only two real mounts
            assert_eq!(disks.len(), 2);
            let root = disks.iter().find(|d| d.mount_point == "/").unwrap();
            assert_eq!(root.total_bytes, 1024 * 1024);
            let data = disks.iter().find(|d| d.mount_point == "/mnt/data disk").unwrap();
            assert_eq!(data.total_bytes, 2048 * 1024);
        }
        CollectionOutcome::Failed(f) => panic!("failed: {:?}", f),
    }
}

#[test]
fn non_linux_platform_fails_nonlinux() {
    let raw = format!("{}", frame("platform", "Darwin\n"));
    let output = CollectorOutput { stdout: raw.clone(), exit_code: Some(0) };
    let outcome = build_collection(&output, None, 1);
    match outcome {
        CollectionOutcome::Failed(f) => assert!(matches!(f.kind, CollectionFailureKind::NonLinux)),
        _ => panic!("expected failed"),
    }
}

#[test]
fn nonzero_exit_fails_script() {
    let raw = String::new();
    let output = CollectorOutput { stdout: raw, exit_code: Some(1) };
    let outcome = build_collection(&output, None, 1);
    match outcome {
        CollectionOutcome::Failed(f) => assert!(matches!(f.kind, CollectionFailureKind::Script)),
        _ => panic!("expected failed"),
    }
}

#[test]
fn missing_df_leaves_disks_none_partial() {
    let raw = format!(
        "{}{}{}{}{}{}{}{}{}",
        frame("boot_id_1", "b1\n"),
        frame("boot_id_2", "b1\n"),
        frame("uptime_1", "10.0\n"),
        frame("uptime_2", "11.0\n"),
        frame("stat_1", "cpu  1 0 1 1 0 0 0 0 0 0\n"),
        frame("stat_2", "cpu  2 0 1 1 0 0 0 0 0 0\n"),
        frame("net_1", "eth0: 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0\n"),
        frame("net_2", "eth0: 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0\n"),
        frame("meminfo", "MemTotal: 1000000 kB\nMemAvailable: 400000 kB\n"),
    );
    let output = CollectorOutput { stdout: raw.clone(), exit_code: Some(0) };
    let outcome = build_collection(&output, None, 1);
    match outcome {
        CollectionOutcome::Partial(s) => assert!(s.disks.is_none()),
        CollectionOutcome::Success(s) => assert!(s.disks.is_none()),
        CollectionOutcome::Failed(f) => panic!("failed: {:?}", f),
    }
}

#[test]
fn no_usable_resource_fails() {
    // only platform frame linux but nothing usable
    let raw = format!("{}", frame("platform", "Linux\n"));
    let output = CollectorOutput { stdout: raw.clone(), exit_code: Some(0) };
    let outcome = build_collection(&output, None, 1);
    match outcome {
        CollectionOutcome::Failed(f) => assert!(matches!(f.kind, CollectionFailureKind::NoUsableResources)),
        _ => panic!("expected failed"),
    }
}

#[test]
fn network_rates_use_actual_uptime_delta() {
    let net1 = "Inter-|   Receive                                                |  Transmit\n face |bytes packets errs drop fifo frame compressed multicast|bytes packets errs drop fifo colls carrier compressed\n    lo: 100 0 0 0 0 0 0 0 200 0 0 0 0 0 0 0\n  eth1: 10 0 0 0 0 0 0 0 20 0 0 0 0 0 0 0\n  eth0: 1000 0 0 0 0 0 0 0 2000 0 0 0 0 0 0 0\n";
    let net2 = "Inter-|   Receive                                                |  Transmit\n face |bytes packets errs drop fifo frame compressed multicast|bytes packets errs drop fifo colls carrier compressed\n    lo: 200 0 0 0 0 0 0 0 400 0 0 0 0 0 0 0\n  eth1: 30 0 0 0 0 0 0 0 50 0 0 0 0 0 0 0\n  eth0: 1100 0 0 0 0 0 0 0 2200 0 0 0 0 0 0 0\n";
    let raw = format!(
        "{}{}{}{}{}{}{}{}{}{}",
        frame("boot_id_1", "b1\n"),
        frame("boot_id_2", "b1\n"),
        frame("uptime_1", "10.0\n"),
        frame("uptime_2", "12.0\n"),
        frame("stat_1", "cpu  1 0 1 1 0 0 0 0 0 0\n"),
        frame("stat_2", "cpu  2 0 1 1 0 0 0 0 0 0\n"),
        frame("net_1", net1),
        frame("net_2", net2),
        frame("meminfo", "MemTotal: 1000000 kB\nMemAvailable: 400000 kB\n"),
        frame("df", "Filesystem 1K-blocks Used Available Use% Mounted on\n/dev/sda1 1000 500 500 50% /\n"),
    );
    let output = CollectorOutput { stdout: raw.clone(), exit_code: Some(0) };
    let outcome = build_collection(&output, Some("eth1"), 1);
    match outcome {
        CollectionOutcome::Success(s) | CollectionOutcome::Partial(s) => {
            let n = s.network.expect("network");
            assert_eq!(n.interface, "eth1");
        }
        CollectionOutcome::Failed(f) => panic!("failed: {:?}", f),
    }
}
