    fn launcher_item(
        id: &str,
        pinned: bool,
        pin_order: u32,
        last_launched_at: Option<u64>,
    ) -> LauncherRecord {
        LauncherRecord {
            id: id.to_string(),
            name: format!("item-{}", id),
            item_type: "script".to_string(),
            target: "echo hello".to_string(),
            pinned,
            pin_order,
            launch_count: 0,
            last_launched_at,
            trusted: false,
            created_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn launcher_sort_prefers_pinned_then_recent() {
        let mut items = vec![
            launcher_item("a", false, 0, Some(100)),
            launcher_item("b", true, 1, Some(1)),
            launcher_item("c", true, 0, Some(50)),
            launcher_item("d", false, 0, Some(200)),
        ];
        sort_launcher_items(&mut items);
        let ids: Vec<String> = items.into_iter().map(|it| it.id).collect();
        assert_eq!(ids, vec!["c", "b", "d", "a"]);
    }

    #[test]
    fn launcher_merge_overwrites_same_id() {
        let mut existing = vec![
            launcher_item("a", false, 0, Some(10)),
            launcher_item("b", false, 0, Some(20)),
        ];
        let mut updated_a = launcher_item("a", true, 0, Some(30));
        updated_a.name = "updated".to_string();
        let new_c = launcher_item("c", false, 0, Some(40));
        merge_launcher_items(&mut existing, vec![updated_a.clone(), new_c.clone()]);
        assert_eq!(existing.len(), 3);
        assert!(existing.iter().any(|it| it.id == "c"));
        let a = existing
            .iter()
            .find(|it| it.id == "a")
            .expect("a should exist");
        assert_eq!(a.name, "updated");
        assert!(a.pinned);
    }

    #[test]
    fn launcher_import_input_defaults() {
        let now = 1000;
        let input = LauncherItemInput {
            id: None,
            name: "docs".to_string(),
            item_type: "url".to_string(),
            target: "https://example.com".to_string(),
            ..LauncherItemInput::default()
        };
        let parsed = launcher_record_from_import_input(input, now)
            .expect("parse launcher input should work");
        assert!(!parsed.id.is_empty());
        assert_eq!(parsed.item_type, "url");
        assert_eq!(parsed.created_at, now);
        assert_eq!(parsed.updated_at, now);
        assert!(parsed.trusted);
    }

    #[test]
    fn normalize_app_target_accepts_open_command() {
        let parsed = normalize_app_target("open -a \"Visual Studio Code\"")
            .expect("should parse open -a form");
        assert_eq!(parsed, "Visual Studio Code");
    }

    #[test]
    fn normalize_app_target_strips_smart_quotes() {
        let parsed = normalize_app_target("open -a “WPS”").expect("should strip smart quotes");
        assert_eq!(parsed, "WPS");
        let parsed2 = normalize_app_target("“微信").expect("should strip leading smart quote");
        assert_eq!(parsed2, "微信");
    }

    #[test]
    fn normalize_icon_candidate_name_adds_icns_extension() {
        assert_eq!(
            normalize_icon_candidate_name("AppIcon"),
            Some("AppIcon.icns".to_string())
        );
        assert_eq!(
            normalize_icon_candidate_name("Foo.icns"),
            Some("Foo.icns".to_string())
        );
    }

    #[test]
    fn extract_icon_candidates_from_plist_json_collects_expected_keys() {
        let plist = json!({
            "CFBundleIconFile": "MainIcon",
            "CFBundleIconName": "NamedIcon",
            "CFBundleIcons": {
                "CFBundlePrimaryIcon": {
                    "CFBundleIconFiles": ["SmallIcon", "LargeIcon"]
                }
            },
            "CFBundleIconFiles": ["FallbackIcon"]
        });

        let candidates = extract_icon_candidates_from_plist_json(&plist);
        assert!(candidates.iter().any(|it| it == "MainIcon.icns"));
        assert!(candidates.iter().any(|it| it == "NamedIcon.icns"));
        assert!(candidates.iter().any(|it| it == "LargeIcon.icns"));
        assert!(candidates.iter().any(|it| it == "FallbackIcon.icns"));
        assert!(candidates.iter().any(|it| it == "AppIcon.icns"));
    }

    #[test]
    fn sync_directory_bidirectional_exports_when_local_is_newer() {
        let root = make_temp_dir("sync-dir-export");
        let local = root.join("local");
        let shared = root.join("shared");
        fs::create_dir_all(&local).expect("create local");
        fs::create_dir_all(&shared).expect("create shared");

        let rel = Path::new("repo").join("skill.md");
        write_test_file(&shared.join(&rel), "shared-old");
        sleep(Duration::from_secs(1));
        write_test_file(&local.join(&rel), "local-new");

        let mut warnings = vec![];
        sync_directory_bidirectional(&local, &shared, &mut warnings, "skills_repository")
            .expect("sync should succeed");

        let synced = fs::read_to_string(shared.join(&rel)).expect("read shared");
        assert_eq!(synced, "local-new");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn sync_directory_bidirectional_imports_when_shared_is_newer() {
        let root = make_temp_dir("sync-dir-import");
        let local = root.join("local");
        let shared = root.join("shared");
        fs::create_dir_all(&local).expect("create local");
        fs::create_dir_all(&shared).expect("create shared");

        let rel = Path::new("meta").join("index.json");
        write_test_file(&local.join(&rel), "local-old");
        sleep(Duration::from_secs(1));
        write_test_file(&shared.join(&rel), "shared-new");

        let mut warnings = vec![];
        sync_directory_bidirectional(&local, &shared, &mut warnings, "skills_repository")
            .expect("sync should succeed");

        let synced = fs::read_to_string(local.join(&rel)).expect("read local");
        assert_eq!(synced, "shared-new");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn sync_directory_bidirectional_copies_single_side_files() {
        let root = make_temp_dir("sync-dir-single-side");
        let local = root.join("local");
        let shared = root.join("shared");
        fs::create_dir_all(&local).expect("create local");
        fs::create_dir_all(&shared).expect("create shared");

        let rel_local_only = Path::new("repository").join("local-only.txt");
        let rel_shared_only = Path::new("meta").join("shared-only.json");
        write_test_file(&local.join(&rel_local_only), "from-local");
        write_test_file(&shared.join(&rel_shared_only), "from-shared");

        let mut warnings = vec![];
        sync_directory_bidirectional(&local, &shared, &mut warnings, "skills_repository")
            .expect("sync should succeed");

        assert_eq!(
            fs::read_to_string(shared.join(&rel_local_only)).expect("read shared copy"),
            "from-local"
        );
        assert_eq!(
            fs::read_to_string(local.join(&rel_shared_only)).expect("read local copy"),
            "from-shared"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn sync_directory_bidirectional_skips_export_when_icloud_placeholder_exists() {
        let root = make_temp_dir("sync-dir-icloud-placeholder");
        let local = root.join("local");
        let shared = root.join("shared");
        fs::create_dir_all(&local).expect("create local");
        fs::create_dir_all(&shared).expect("create shared");

        let rel = Path::new("repository").join("pending-skill.md");
        write_test_file(&local.join(&rel), "local-content");
        write_test_file(
            &shared.join("repository").join("pending-skill.md.icloud"),
            "",
        );

        let mut warnings = vec![];
        sync_directory_bidirectional(&local, &shared, &mut warnings, "skills_repository")
            .expect("sync should succeed");

        assert!(!shared.join(&rel).exists());
        assert!(warnings
            .iter()
            .any(|w| w.contains("shared file pending download")));
        assert!(warnings
            .iter()
            .any(|w| w.contains("skip exporting while shared file is pending download")));

        let _ = fs::remove_dir_all(&root);
    }
