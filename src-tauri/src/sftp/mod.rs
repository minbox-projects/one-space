mod directory;
mod log_tail;
mod paths;
mod session;
mod transfer;
mod trash;

pub use directory::*;
pub use log_tail::*;
pub use paths::*;
#[cfg(test)]
use std::path::PathBuf;
#[cfg(test)]
use std::time::{Duration, Instant};
pub use transfer::*;
pub use trash::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quote_remote_path() {
        assert_eq!(quote_remote_path("~"), "\"$HOME\"");
        assert_eq!(quote_remote_path("~/"), "\"$HOME/\"");
        assert_eq!(quote_remote_path("~/logs"), "\"$HOME/\"'logs'");
        assert_eq!(quote_remote_path("/var/log"), "'/var/log'");
        assert_eq!(quote_remote_path("dir's name"), "'dir'\\''s name'");
    }

    #[test]
    fn test_parse_ls_output() {
        let sample_output = r#"/var/log
total 24
drwxr-xr-x  3 root root 4096 2026-10-21 14:30 .
drwxr-xr-x 10 root root 4096 2026-10-20 09:12 ..
drwxr-xr-x  2 root root 4096 2026-10-21 14:30 nginx
-rw-r--r--  1 root root 1024 2026-10-21 15:20 app.log
-rw-r--r--  1 root root  512 2026-10-20 11:00 config.yml
"#;

        let res = parse_ls_output("~", sample_output);
        assert_eq!(res.current_path, "/var/log");
        assert_eq!(res.entries.len(), 3);

        // 目录在前
        assert_eq!(res.entries[0].name, "nginx");
        assert!(res.entries[0].is_dir);
        assert_eq!(res.entries[0].path, "/var/log/nginx");

        // 文件在后
        assert_eq!(res.entries[1].name, "app.log");
        assert!(!res.entries[1].is_dir);
        assert_eq!(res.entries[1].size, 1024);
        assert_eq!(res.entries[1].path, "/var/log/app.log");

        assert_eq!(res.entries[2].name, "config.yml");
    }

    #[test]
    fn test_calculate_progress_metrics() {
        let now = Instant::now();
        // 初始传输 50MB / 100MB
        let (speed, eta, pct) = calculate_progress_metrics(
            50 * 1024 * 1024,
            100 * 1024 * 1024,
            now - Duration::from_secs(5),
            0,
            now - Duration::from_secs(5),
        );
        assert!((pct - 50.0).abs() < 0.1);
        assert!(speed > 0);
        assert!(eta.is_some());
        // 传输完毕 100MB / 100MB
        let (_, eta_done, pct_done) = calculate_progress_metrics(
            100 * 1024 * 1024,
            100 * 1024 * 1024,
            now - Duration::from_secs(10),
            90 * 1024 * 1024,
            now - Duration::from_secs(1),
        );
        assert_eq!(pct_done, 100.0);
        assert_eq!(eta_done, Some(0));
    }

    #[test]
    fn test_expand_local_path() {
        let p = expand_local_path("/var/log");
        assert_eq!(p, PathBuf::from("/var/log"));

        let p_home = expand_local_path("~/Downloads/pelican-cycling.html");
        assert!(!p_home.to_string_lossy().starts_with("~"));
        assert!(p_home.to_string_lossy().ends_with("Downloads/pelican-cycling.html"));
    }

    #[tokio::test]
    async fn test_delete_entry_safety_checks() {
        assert!(sftp_delete_entry("host".to_string(), "/".to_string(), None).await.is_err());
        assert!(sftp_delete_entry("host".to_string(), "~".to_string(), None).await.is_err());
        assert!(sftp_delete_entry("host".to_string(), "$HOME".to_string(), None).await.is_err());
        assert!(sftp_delete_entry("host".to_string(), ".".to_string(), None).await.is_err());
        assert!(sftp_delete_entry("host".to_string(), "..".to_string(), None).await.is_err());
        assert!(sftp_delete_entry("host".to_string(), "   ".to_string(), None).await.is_err());
    }

    #[tokio::test]
    async fn test_restore_entry_validation() {
        assert!(sftp_restore_entry("host".to_string(), "".to_string(), "/var/log".to_string()).await.is_err());
        assert!(sftp_restore_entry("host".to_string(), "~/.onespace_trash/1_a.log".to_string(), "".to_string()).await.is_err());
    }

    #[test]
    fn test_parse_trash_output() {
        let sample_trash = r#"===MANIFEST===
1729587400_app.log	/var/log/nginx/app.log	1729587400
1729587500_data	/data/backup/data	1729587500
===LS===
total 16
drwxr-xr-x 2 root root 4096 2026-10-22 09:10 .
drwxr-xr-x 5 root root 4096 2026-10-22 09:00 ..
-rw-r--r-- 1 root root  120 2026-10-22 09:12 .manifest
-rw-r--r-- 1 root root 8192 2026-10-22 09:10 1729587400_app.log
drwxr-xr-x 2 root root 4096 2026-10-22 09:11 1729587500_data
-rw-r--r-- 1 root root 1024 2026-10-22 09:15 1729587600_orphan.txt
"#;
        let items = parse_trash_output(sample_trash);
        assert_eq!(items.len(), 3);

        // 第一个（倒序排序）：1729587600_orphan.txt（无 manifest 兜底）
        assert_eq!(items[0].id, "1729587600_orphan.txt");
        assert_eq!(items[0].name, "orphan.txt");
        assert_eq!(items[0].original_path, "~/orphan.txt");
        assert_eq!(items[0].size, 1024);
        assert!(!items[0].is_dir);

        // 第二个：1729587500_data（来自 manifest，目录）
        assert_eq!(items[1].id, "1729587500_data");
        assert_eq!(items[1].name, "data");
        assert_eq!(items[1].original_path, "/data/backup/data");
        assert!(items[1].is_dir);

        // 第三个：1729587400_app.log（来自 manifest，文件）
        assert_eq!(items[2].id, "1729587400_app.log");
        assert_eq!(items[2].name, "app.log");
        assert_eq!(items[2].original_path, "/var/log/nginx/app.log");
        assert_eq!(items[2].size, 8192);
        assert!(!items[2].is_dir);
    }

    #[tokio::test]
    async fn test_restore_trash_item_validation() {
        assert!(sftp_restore_trash_item("host".to_string(), "".to_string()).await.is_err());
        assert!(sftp_restore_trash_item("host".to_string(), "../escape".to_string()).await.is_err());
        assert!(sftp_restore_trash_item("host".to_string(), "dir/sub".to_string()).await.is_err());
    }
}
