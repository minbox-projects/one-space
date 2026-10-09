use super::*;
use crate::app_store::{provider_activation, session_service};
use std::process::{Command, Stdio};
use std::time::Instant;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn opencode_provider(id: &str, name: &str) -> ServiceProviderRecord {
    ServiceProviderRecord {
        id: id.to_string(),
        name: name.to_string(),
        tool: "opencode".to_string(),
        ..ServiceProviderRecord::default()
    }
}

fn codex_provider(id: &str, name: &str) -> ServiceProviderRecord {
    ServiceProviderRecord {
        id: id.to_string(),
        name: name.to_string(),
        tool: "codex".to_string(),
        ..ServiceProviderRecord::default()
    }
}

#[cfg(unix)]
fn claude_session(working_dir: &Path, name: &str) -> SessionInput {
    SessionInput {
        id: None,
        name: name.to_string(),
        working_dir: working_dir.to_string_lossy().to_string(),
        tool: "claude".to_string(),
        tool_session_id: None,
        runtime_mode: None,
        runtime_profile_id: None,
        preset_id: None,
        status: None,
        provider_id: None,
        initial_prompt: None,
        permission_mode: None,
    }
}

#[cfg(unix)]
fn write_executable(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create stub parent");
    }
    fs::write(path, content).expect("write stub");
    let mut perms = fs::metadata(path).expect("stub metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).expect("chmod stub");
}

#[cfg(unix)]
struct PathRestore(Option<String>);

#[cfg(unix)]
impl Drop for PathRestore {
    fn drop(&mut self) {
        match self.0.take() {
            Some(value) => std::env::set_var("PATH", value),
            None => std::env::remove_var("PATH"),
        }
    }
}

/// Temporarily point the child-process search path at isolated stubs. The guard
/// restores the previous value on drop, including during an assertion unwind.
#[cfg(unix)]
fn set_path(value: &str) -> PathRestore {
    let previous = std::env::var("PATH").ok();
    std::env::set_var("PATH", value);
    PathRestore(previous)
}

/// AC-001 OpenCode active-set consistency: the real internal CLI `env use`
/// entry point must add to the OpenCode active set (never replace it), leave
/// other tools' single-active bindings untouched, and never project into
/// tool-side configuration. The public provider listing is the observable.
///
/// RED before activation production fix: `use_cli_environment` still writes the
/// legacy single `active["opencode"]` slot instead of appending
/// `active_opencode`, so the listing stays `[A]` instead of `[A, B]`.
#[test]
fn cli_env_use_appends_opencode_active_set_without_projection() {
    with_temp_dir("cli-env-use-opencode", |home| {
        let id_a = uuid::Uuid::new_v4().to_string();
        let id_b = uuid::Uuid::new_v4().to_string();
        let id_c = uuid::Uuid::new_v4().to_string();

        let mut state = ServiceProvidersState::default();
        state.providers = vec![
            opencode_provider(&id_a, "OpenCode Alpha"),
            opencode_provider(&id_b, "OpenCode Beta"),
            codex_provider(&id_c, "Codex Main"),
        ];
        state.active.insert("codex".to_string(), id_c.clone());
        state.active_opencode = vec![id_a.clone()];
        save_service_providers_internal(&state).expect("seed canonical providers");

        // Sentinel tool-side configuration that env use must not rewrite.
        let opencode_config = home.join(".config").join("opencode").join("opencode.json");
        write_test_file(&opencode_config, "{\"sentinel\":\"keep\"}");

        // The same function the installed `onespace env use` script invokes.
        let selected =
            provider_activation::use_cli_environment("opencode", "OpenCode Beta").expect("env use");
        assert_eq!(selected, id_b, "env use resolves the provider by name");

        let payload = service_providers_list().expect("public provider listing").data;
        let active_opencode: Vec<String> = payload["active_opencode"]
            .as_array()
            .expect("active_opencode array")
            .iter()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect();
        assert_eq!(
            active_opencode,
            vec![id_a.clone(), id_b.clone()],
            "CLI env use must add to the OpenCode active set, not replace it"
        );

        // Other tools retain their single-active binding.
        assert_eq!(
            payload["active"]["codex"].as_str(),
            Some(id_c.as_str()),
            "activating OpenCode must not change another tool's active provider"
        );

        // env use must not project into the OpenCode config.
        assert_eq!(
            fs::read_to_string(&opencode_config).expect("read sentinel"),
            "{\"sentinel\":\"keep\"}",
            "env use must not project CLI configuration"
        );

        // Unknown target rejects without mutating any active binding.
        let before = load_service_providers_state().expect("reload before unknown target");
        let error = provider_activation::use_cli_environment("opencode", "missing-provider")
            .expect_err("unknown provider must be rejected");
        assert!(!error.is_empty(), "unknown provider error must be actionable");
        let after = load_service_providers_state().expect("reload after unknown target");
        assert_eq!(after.active_opencode, before.active_opencode);
        assert_eq!(after.active, before.active);
    });
}

/// AC-001 creation integration (characterization of the delivered production
/// service; not a claimed initial RED): the real
/// `create_session_in_current_terminal` must launch the native command in the
/// invoking working directory with unchanged argv while publishing exactly one
/// encrypted canonical pending record, and the display name must be preserved
/// verbatim (quotes included) rather than used as a native ID.
#[cfg(unix)]
#[test]
fn create_in_current_terminal_publishes_encrypted_pending_record_with_launch_argv() {
    with_temp_dir("cli-create-integration", |home| {
        let working_dir = home.join("project");
        fs::create_dir_all(&working_dir).expect("create working dir");
        let stub_bin = home.join("stub-bin");
        fs::create_dir_all(&stub_bin).expect("create stub bin");

        let argv_log = home.join("native-argv.log");
        let cwd_log = home.join("native-cwd.log");
        write_executable(
            &stub_bin.join("claude"),
            &format!(
                "#!/usr/bin/env bash\nprintf '%s\\n' \"$@\" > '{argv}'\npwd > '{cwd}'\nexit 0\n",
                argv = argv_log.display(),
                cwd = cwd_log.display()
            ),
        );
        let _path = set_path(&format!("{}:/usr/bin:/bin", stub_bin.display()));

        let name = "my project's \"odd\" name";
        let session = claude_session(&working_dir, name);
        let (response, status) = tauri::async_runtime::block_on(
            session_service::create_session_in_current_terminal(
                session,
                &["--model".to_string(), "sonnet".to_string()],
            ),
        )
        .expect("create in current terminal");

        let record = response.data;
        assert_eq!(record.tool, "claude");
        assert_eq!(record.name, name, "display name must be preserved verbatim");
        assert_eq!(record.status, "pending_bind");
        assert!(
            record.tool_session_id.is_empty(),
            "a pending record must not carry an invented native ID"
        );
        assert!(status.success(), "stub native tool exits 0");

        // The native child runs with the prepared argv plus the caller's argv,
        // in the canonical working directory.
        let argv = fs::read_to_string(&argv_log).expect("native argv captured");
        assert!(argv.contains("--session-id"), "native argv: {argv:?}");
        assert!(argv.contains("--model"), "native argv: {argv:?}");
        assert!(argv.contains("sonnet"), "native argv: {argv:?}");
        let cwd = fs::read_to_string(&cwd_log).expect("native cwd captured");
        assert_eq!(cwd.trim(), record.working_dir);

        // Canonical state is encrypted at rest and publicly listed.
        let store_path = StorageEngine::sessions_path().expect("sessions path");
        let raw = fs::read_to_string(&store_path).expect("read sessions store");
        let blob: Value = serde_json::from_str(&raw).expect("sessions store is an encrypted blob");
        assert_eq!(
            blob["is_encrypted"].as_bool(),
            Some(true),
            "canonical sessions store must be encrypted at rest"
        );
        assert!(
            !raw.contains(name),
            "plaintext display name must not appear in the encrypted store"
        );

        let listed = sessions_list().expect("public sessions listing").data;
        let entry = listed
            .iter()
            .find(|value| value["name"].as_str() == Some(name))
            .expect("created session must be publicly listed");
        assert_eq!(entry["status"].as_str(), Some("pending_bind"));
        assert_eq!(entry["tool_session_id"].as_str(), Some(""));
    });
}

/// AC-001 failure boundary (characterization): a native spawn failure rolls the
/// pending record back, while a successfully spawned child that exits nonzero is
/// a result — the record stays pending and recoverable.
#[cfg(unix)]
#[test]
fn create_in_current_terminal_spawn_failure_rolls_back_but_nonzero_child_is_pending() {
    with_temp_dir("cli-create-failure-vs-nonzero", |home| {
        let working_dir = home.join("project");
        fs::create_dir_all(&working_dir).expect("create working dir");

        // Spawn failure: no native tool is reachable on an isolated PATH.
        {
            let empty_bin = home.join("empty-bin");
            fs::create_dir_all(&empty_bin).expect("create empty bin");
            let _path = set_path(&empty_bin.to_string_lossy());
            let error = tauri::async_runtime::block_on(
                session_service::create_session_in_current_terminal(
                    claude_session(&working_dir, "spawn should fail"),
                    &[],
                ),
            )
            .expect_err("missing native tool must fail the create");
            assert_eq!(error.code, "launch_failed");
        }
        assert!(
            load_sessions_state()
                .expect("load after spawn failure")
                .sessions
                .is_empty(),
            "a failed spawn must not leave a recoverable pending record"
        );

        // Real nonzero child: registration remains pending and recoverable.
        let stub_bin = home.join("nonzero-bin");
        fs::create_dir_all(&stub_bin).expect("create stub bin");
        write_executable(
            &stub_bin.join("claude"),
            "#!/usr/bin/env bash\nexit 7\n",
        );
        let _path = set_path(&format!("{}:/usr/bin:/bin", stub_bin.display()));
        let (response, status) = tauri::async_runtime::block_on(
            session_service::create_session_in_current_terminal(
                claude_session(&working_dir, "nonzero child"),
                &[],
            ),
        )
        .expect("a started nonzero child is a result, not a spawn error");
        assert_eq!(status.code(), Some(7));
        assert_eq!(response.data.status, "pending_bind");
        assert!(response.data.tool_session_id.is_empty());

        let persisted = load_sessions_state().expect("load after nonzero child");
        assert_eq!(persisted.sessions.len(), 1);
        assert_eq!(persisted.sessions[0].name, "nonzero child");
        assert_eq!(persisted.sessions[0].status, "pending_bind");
    });
}

/// AC-001 env listing characterization (expected GREEN): the public `env list`
/// rendering reflects the full canonical OpenCode multiset and never falls back
/// to a stale legacy single slot when the canonical set is empty.
#[test]
fn cli_environment_listing_reports_opencode_multiset_and_ignores_legacy_slot() {
    with_temp_dir("cli-env-listing-multiple", |_home| {
        let id_a = uuid::Uuid::new_v4().to_string();
        let id_b = uuid::Uuid::new_v4().to_string();
        let mut state = ServiceProvidersState::default();
        state.providers = vec![
            opencode_provider(&id_a, "Alpha"),
            opencode_provider(&id_b, "Beta"),
        ];
        state.active_opencode = vec![id_a.clone(), id_b.clone()];
        save_service_providers_internal(&state).expect("seed canonical providers");

        let listing = provider_activation::cli_environment_listing().expect("env listing");
        let active_section = listing
            .split("Current Active:")
            .nth(1)
            .unwrap_or_default();
        assert!(active_section.contains("opencode -> Alpha"), "listing: {listing:?}");
        assert!(active_section.contains("opencode -> Beta"), "listing: {listing:?}");
    });

    with_temp_dir("cli-env-listing-empty-legacy", |_home| {
        let id_a = uuid::Uuid::new_v4().to_string();
        let mut state = ServiceProvidersState::default();
        state.providers = vec![opencode_provider(&id_a, "Alpha")];
        state.active_opencode = vec![];
        // Stale legacy single slot that must be ignored for OpenCode.
        state.active.insert("opencode".to_string(), id_a.clone());
        save_service_providers_internal(&state).expect("seed canonical providers");

        let listing = provider_activation::cli_environment_listing().expect("env listing");
        let active_section = listing
            .split("Current Active:")
            .nth(1)
            .unwrap_or_default();
        assert!(
            !active_section.contains("opencode -> Alpha"),
            "empty canonical OpenCode set must not fall back to the legacy slot: {listing:?}"
        );
    });
}

/// AC-001 activation convergence characterization (expected GREEN): GUI
/// `activate_provider` and CLI `use_cli_environment` write the same canonical
/// OpenCode multiset, and repeated CLI selection is idempotent.
#[test]
fn cli_and_gui_opencode_activation_converge_without_duplicates() {
    with_temp_dir("cli-gui-activation-converge", |_home| {
        let id_a = uuid::Uuid::new_v4().to_string();
        let id_b = uuid::Uuid::new_v4().to_string();
        let mut state = ServiceProvidersState::default();
        state.providers = vec![
            opencode_provider(&id_a, "Alpha"),
            opencode_provider(&id_b, "Beta"),
        ];
        save_service_providers_internal(&state).expect("seed canonical providers");

        provider_activation::activate_provider("opencode", &id_b).expect("gui activate Beta");
        let selected = provider_activation::use_cli_environment("opencode", "Alpha")
            .expect("cli env use Alpha");
        assert_eq!(selected, id_a);
        provider_activation::use_cli_environment("opencode", "Beta")
            .expect("cli env use Beta again is idempotent");

        let canonical = load_service_providers_state().expect("reload canonical providers");
        let mut ids = canonical.active_opencode.clone();
        ids.sort();
        let mut expected = vec![id_a.clone(), id_b.clone()];
        expected.sort();
        assert_eq!(
            ids, expected,
            "GUI and CLI must share one canonical active set"
        );
        assert_eq!(canonical.active_opencode.len(), 2, "no duplicate activations");
    });
}

const CONCURRENCY_WORKER_TEST: &str =
    "app_store::tests::cli_workflows::concurrency_worker_opencode_activation";

fn wait_for_file(path: &Path, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        sleep(Duration::from_millis(5));
    }
    path.exists()
}

fn wait_for_either(first: &Path, second: &Path, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if first.exists() || second.exists() {
            return true;
        }
        sleep(Duration::from_millis(5));
    }
    first.exists() || second.exists()
}

/// Child-process worker for the AC-001 concurrent-writer regression. It is not
/// a standalone test: the parent spawns the current test executable with
/// `--ignored` and the ONESPACE_CONCURRENCY_* environment. Without that
/// environment it is a no-op, so ordinary runs never execute it.
#[test]
#[ignore]
fn concurrency_worker_opencode_activation() {
    let Ok(role) = std::env::var("ONESPACE_CONCURRENCY_ROLE") else {
        return;
    };
    let barrier =
        PathBuf::from(std::env::var("ONESPACE_CONCURRENCY_BARRIER").expect("barrier path"));
    let provider_id =
        std::env::var("ONESPACE_CONCURRENCY_PROVIDER_ID").expect("provider id");

    fs::write(barrier.join(format!("ready-{role}")), b"").expect("write ready marker");
    assert!(
        wait_for_file(&barrier.join("go"), Duration::from_secs(30)),
        "worker {role} timed out waiting for the go barrier"
    );

    match role.as_str() {
        "gui" => {
            provider_activation::activate_provider("opencode", &provider_id)
                .map(|_| ())
                .expect("gui activate_provider must succeed");
        }
        "cli" => {
            provider_activation::use_cli_environment("opencode", &provider_id)
                .map(|_| ())
                .expect("cli use_cli_environment must succeed");
        }
        other => panic!("unknown concurrency worker role: {other}"),
    }
}

/// AC-001 concurrent independent writers: two separate processes call the real
/// GUI activation service and the real CLI `env use` for different OpenCode
/// providers against one shared isolated canonical store. Both must report
/// success, the public listing must retain both activations, and the encrypted
/// file must remain complete and decryptable. Uses only public services and
/// confined fixture HOME/barrier files; no lock or crypto internals are mocked.
#[test]
fn concurrent_gui_and_cli_opencode_activation_keep_both_changes() {
    with_temp_dir("concurrent-gui-cli-opencode", |home| {
        let id_a = uuid::Uuid::new_v4().to_string();
        let id_b = uuid::Uuid::new_v4().to_string();

        let mut seeded = ServiceProvidersState::default();
        seeded.providers = vec![
            opencode_provider(&id_a, "Alpha"),
            opencode_provider(&id_b, "Beta"),
        ];
        save_service_providers_internal(&seeded).expect("seed shared canonical providers");

        let barrier = home.join("concurrency-barrier");
        fs::create_dir_all(&barrier).expect("create barrier dir");

        let exe = std::env::current_exe().expect("current test executable");
        let spawn_worker = |role: &str, provider_id: &str| {
            Command::new(&exe)
                .args([
                    "--exact",
                    CONCURRENCY_WORKER_TEST,
                    "--ignored",
                    "--nocapture",
                ])
                .env("HOME", home)
                .env("ONESPACE_CONCURRENCY_ROLE", role)
                .env("ONESPACE_CONCURRENCY_BARRIER", &barrier)
                .env("ONESPACE_CONCURRENCY_PROVIDER_ID", provider_id)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("spawn worker process")
        };

        let gui = spawn_worker("gui", &id_a);
        let cli = spawn_worker("cli", &id_b);

        assert!(
            wait_for_file(&barrier.join("ready-gui"), Duration::from_secs(30)),
            "gui worker never became ready"
        );
        assert!(
            wait_for_file(&barrier.join("ready-cli"), Duration::from_secs(30)),
            "cli worker never became ready"
        );
        fs::write(barrier.join("go"), b"").expect("release concurrency barrier");

        let gui_out = gui.wait_with_output().expect("wait gui worker");
        let cli_out = cli.wait_with_output().expect("wait cli worker");

        assert!(
            gui_out.status.success(),
            "GUI activation worker failed (status {:?})\nstdout:\n{}\nstderr:\n{}",
            gui_out.status,
            String::from_utf8_lossy(&gui_out.stdout),
            String::from_utf8_lossy(&gui_out.stderr)
        );
        assert!(
            cli_out.status.success(),
            "CLI env use worker failed (status {:?})\nstdout:\n{}\nstderr:\n{}",
            cli_out.status,
            String::from_utf8_lossy(&cli_out.stdout),
            String::from_utf8_lossy(&cli_out.stderr)
        );
        eprintln!(
            "concurrency workers: gui status={:?} stdout={:?} stderr={:?} | cli status={:?} stdout={:?} stderr={:?}",
            gui_out.status,
            String::from_utf8_lossy(&gui_out.stdout),
            String::from_utf8_lossy(&gui_out.stderr),
            cli_out.status,
            String::from_utf8_lossy(&cli_out.stdout),
            String::from_utf8_lossy(&cli_out.stderr),
        );

        let payload = service_providers_list()
            .expect("public provider listing after concurrent writes")
            .data;
        let mut ids: Vec<String> = payload["active_opencode"]
            .as_array()
            .expect("active_opencode array")
            .iter()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect();
        ids.sort();
        let mut expected = vec![id_a.clone(), id_b.clone()];
        expected.sort();
        assert_eq!(
            ids, expected,
            "both independent OpenCode activations must survive in canonical state"
        );

        let providers_path = StorageEngine::providers_path().expect("providers path");
        let raw = fs::read_to_string(&providers_path).expect("read providers store");
        let blob: EncryptedBlob = serde_json::from_str(&raw).expect("complete encrypted blob");
        assert!(blob.is_encrypted, "providers store must be encrypted");
        assert!(!blob.data.is_empty(), "encrypted payload must not be empty");
        let decrypted =
            load_service_providers_state().expect("canonical providers store must decrypt");
        assert_eq!(decrypted.active_opencode.len(), 2);
    });
}

#[cfg(unix)]
const REGISTRATION_WORKER_TEST: &str =
    "app_store::tests::cli_workflows::registration_worker_create_session";

/// Isolated native stub: records argv/cwd, signals launch, then waits on a
/// fixture release file so the parent can observe the concurrent canonical
/// registration window before the native child exits.
#[cfg(unix)]
fn registration_stub_script() -> String {
    "#!/usr/bin/env bash\nprintf '%s\\n' \"$@\" > \"$ONESPACE_STUB_ARGV_LOG\"\npwd > \"$ONESPACE_STUB_CWD_LOG\"\n: > \"$ONESPACE_STUB_LAUNCHED\"\ni=0\nwhile [ ! -f \"$ONESPACE_STUB_RELEASE\" ] && [ \"$i\" -lt 3000 ]; do sleep 0.01; i=$((i + 1)); done\nexit 0\n".to_string()
}

/// Child-process worker for the AC-001 concurrent-registration
/// characterization. Not a standalone test: the parent spawns the current test
/// executable with `--ignored` and the ONESPACE_REG_* environment; without that
/// environment it is a no-op. Uses only the real public session creation service.
#[cfg(unix)]
#[test]
#[ignore]
fn registration_worker_create_session() {
    let Ok(role) = std::env::var("ONESPACE_REG_ROLE") else {
        return;
    };
    let barrier = PathBuf::from(std::env::var("ONESPACE_REG_BARRIER").expect("barrier path"));
    let go_name = std::env::var("ONESPACE_REG_GO").expect("go barrier name");
    let working_dir = PathBuf::from(std::env::var("ONESPACE_REG_CWD").expect("cwd"));
    let name = std::env::var("ONESPACE_REG_NAME").expect("session name");

    fs::write(barrier.join(format!("ready-{role}")), b"").expect("write ready marker");
    assert!(
        wait_for_file(&barrier.join(&go_name), Duration::from_secs(30)),
        "worker {role} timed out waiting for {go_name}"
    );

    fs::write(barrier.join(format!("calling-{role}")), b"").expect("write calling marker");
    let result = tauri::async_runtime::block_on(
        session_service::create_session_in_current_terminal(claude_session(&working_dir, &name), &[]),
    );
    match role.as_str() {
        "fail" => match result {
            Ok(_) => panic!("spawn-failing registration unexpectedly succeeded"),
            Err(error) => {
                assert_eq!(error.code, "launch_failed");
                fs::write(barrier.join("failed-fail"), b"").expect("write failed marker");
            }
        },
        _ => match result {
            Ok((response, status)) => {
                assert!(status.success(), "native stub exits 0");
                assert_eq!(response.data.name, name);
                assert_eq!(response.data.status, "pending_bind");
                assert!(response.data.tool_session_id.is_empty());
                fs::write(barrier.join(format!("done-{role}")), b"").expect("write done marker");
            }
            Err(error) => {
                fs::write(
                    barrier.join(format!("error-{role}")),
                    format!("{}: {}", error.code, error.message),
                )
                .expect("write error marker");
                panic!("{role} registration failed: {}: {}", error.code, error.message);
            }
        },
    }
}

/// Owns the registration worker processes and their native-stub release files
/// so a failed assertion always unblocks held stubs and reaps only test-owned
/// processes instead of leaving cargo hanging on children.
#[cfg(unix)]
struct RegistrationFixture {
    releases: Vec<PathBuf>,
    children: Vec<(String, Option<std::process::Child>)>,
}

#[cfg(unix)]
impl RegistrationFixture {
    fn new() -> Self {
        Self {
            releases: Vec::new(),
            children: Vec::new(),
        }
    }

    fn track(&mut self, role: &str, release: &Path, child: std::process::Child) {
        self.releases.push(release.to_path_buf());
        self.children.push((role.to_string(), Some(child)));
    }

    fn release_all(&self) {
        for release in &self.releases {
            let _ = fs::write(release, b"");
        }
    }

    fn take(&mut self, role: &str) -> std::process::Output {
        let slot = self
            .children
            .iter_mut()
            .find(|(name, _)| name == role)
            .unwrap_or_else(|| panic!("unknown registration worker role {role}"));
        slot.1
            .take()
            .expect("registration worker already reaped")
            .wait_with_output()
            .expect("wait registration worker")
    }
}

#[cfg(unix)]
impl Drop for RegistrationFixture {
    fn drop(&mut self) {
        self.release_all();
        for (_role, slot) in self.children.iter_mut() {
            if let Some(child) = slot.as_mut() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

#[cfg(unix)]
fn panic_with_fixture_outputs(fixture: &mut RegistrationFixture, message: String) -> ! {
    fixture.release_all();
    let mut report = String::new();
    for (role, slot) in fixture.children.iter_mut() {
        if let Some(child) = slot.take() {
            match child.wait_with_output() {
                Ok(output) => report.push_str(&format!(
                    "\n--- {role} status={:?}\nstdout:\n{}\nstderr:\n{}",
                    output.status,
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                )),
                Err(error) => report.push_str(&format!("\n--- {role} wait failed: {error}")),
            }
        }
    }
    panic!("{message}{report}");
}

/// AC-001 concurrent independent registration characterization: two separate
/// processes register distinct named sessions through the real public
/// `create_session_in_current_terminal` against one shared isolated canonical
/// store. Public `sessions_list` must retain BOTH records and the store must stay
/// a complete decryptable encrypted file. The same barrier is then reused for a
/// spawn-failing registration concurrent with a successful one to pin the
/// rollback-preservation boundary (the failed registration must not remove
/// another process's record). No lock/storage internals are mocked or asserted.
#[cfg(unix)]
#[test]
fn concurrent_independent_registrations_preserve_sessions_and_rollback() {
    with_temp_dir("concurrent-registrations", |home| {
        // Fully migrate once and seed canonical key/state before spawning, so
        // concurrent child migration cannot be mistaken for native-lifetime
        // serialization. This uses the exposed production migration API, not a
        // lock or storage mock.
        run_migration_impl().expect("seed migration state");
        save_service_providers_internal(&ServiceProvidersState::default())
            .expect("seed canonical providers");
        save_sessions_state(&SessionsState::default()).expect("seed canonical sessions");

        let stub_bin = home.join("reg-stub-bin");
        write_executable(&stub_bin.join("claude"), &registration_stub_script());
        let stub_path = format!("{}:/usr/bin:/bin", stub_bin.display());
        let empty_bin = home.join("reg-empty-bin");
        fs::create_dir_all(&empty_bin).expect("create empty bin dir");
        let fail_path = format!("{}:/usr/bin:/bin", empty_bin.display());

        let barrier = home.join("reg-barrier");
        fs::create_dir_all(&barrier).expect("create barrier dir");
        let exe = std::env::current_exe().expect("current test executable");

        let spawn = |role: &str,
                     go: &str,
                     cwd: &Path,
                     name: &str,
                     path: &str,
                     argv_log: &Path,
                     pwd_log: &Path,
                     launched: &Path,
                     release: &Path|
         -> std::process::Child {
            Command::new(&exe)
                .args(["--exact", REGISTRATION_WORKER_TEST, "--ignored", "--nocapture"])
                .env("HOME", home)
                .env("PATH", path)
                .env("ONESPACE_REG_ROLE", role)
                .env("ONESPACE_REG_BARRIER", &barrier)
                .env("ONESPACE_REG_GO", go)
                .env("ONESPACE_REG_CWD", cwd)
                .env("ONESPACE_REG_NAME", name)
                .env("ONESPACE_STUB_ARGV_LOG", argv_log)
                .env("ONESPACE_STUB_CWD_LOG", pwd_log)
                .env("ONESPACE_STUB_LAUNCHED", launched)
                .env("ONESPACE_STUB_RELEASE", release)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("spawn registration worker")
        };

        let gui_cwd = home.join("gui-cwd");
        let cli_cwd = home.join("cli-cwd");
        let ok2_cwd = home.join("ok2-cwd");
        let fail_cwd = home.join("fail-cwd");
        for dir in [&gui_cwd, &cli_cwd, &ok2_cwd, &fail_cwd] {
            fs::create_dir_all(dir).expect("create worker cwd");
        }
        let gui_logs = (
            home.join("gui-argv.log"),
            home.join("gui-pwd.log"),
            home.join("gui-launched"),
            home.join("gui-release"),
        );
        let cli_logs = (
            home.join("cli-argv.log"),
            home.join("cli-pwd.log"),
            home.join("cli-launched"),
            home.join("cli-release"),
        );
        let ok2_logs = (
            home.join("ok2-argv.log"),
            home.join("ok2-pwd.log"),
            home.join("ok2-launched"),
            home.join("ok2-release"),
        );
        let fail_logs = (
            home.join("fail-argv.log"),
            home.join("fail-pwd.log"),
            home.join("fail-launched"),
            home.join("fail-release"),
        );

        // ---- Phase 1: two independent successful registrations.
        let mut fixture = RegistrationFixture::new();
        let gui = spawn(
            "gui", "go1", &gui_cwd, "GUI Session", &stub_path, &gui_logs.0, &gui_logs.1,
            &gui_logs.2, &gui_logs.3,
        );
        fixture.track("gui", &gui_logs.3, gui);
        let cli = spawn(
            "cli", "go1", &cli_cwd, "CLI Session", &stub_path, &cli_logs.0, &cli_logs.1,
            &cli_logs.2, &cli_logs.3,
        );
        fixture.track("cli", &cli_logs.3, cli);

        assert!(
            wait_for_file(&barrier.join("ready-gui"), Duration::from_secs(30)),
            "gui worker never became ready"
        );
        assert!(
            wait_for_file(&barrier.join("ready-cli"), Duration::from_secs(30)),
            "cli worker never became ready"
        );
        fs::write(barrier.join("go1"), b"").expect("release phase 1 barrier");
        assert!(
            wait_for_file(&barrier.join("calling-gui"), Duration::from_secs(30)),
            "gui worker never reached create_session"
        );
        assert!(
            wait_for_file(&barrier.join("calling-cli"), Duration::from_secs(30)),
            "cli worker never reached create_session"
        );

        // BOTH native stubs must be held concurrently before either is released.
        // Users keep native sessions open for a long time, so a second process
        // must reach its own launch while the first is still running.
        let gui_launched =
            wait_for_either(&gui_logs.2, &barrier.join("error-gui"), Duration::from_secs(30));
        let cli_launched =
            wait_for_either(&cli_logs.2, &barrier.join("error-cli"), Duration::from_secs(30));
        if !gui_launched
            || !cli_launched
            || barrier.join("error-gui").exists()
            || barrier.join("error-cli").exists()
        {
            panic_with_fixture_outputs(
                &mut fixture,
                format!(
                    "phase 1: both registrations must launch while the other native stub is held; gui_launched={gui_launched} cli_launched={cli_launched} gui_error={:?} cli_error={:?}",
                    fs::read_to_string(barrier.join("error-gui")).unwrap_or_default(),
                    fs::read_to_string(barrier.join("error-cli")).unwrap_or_default(),
                ),
            );
        }
        fixture.release_all();
        let gui_out = fixture.take("gui");
        let cli_out = fixture.take("cli");
        assert!(
            gui_out.status.success(),
            "GUI registration worker failed (status {:?})\nstdout:\n{}\nstderr:\n{}",
            gui_out.status,
            String::from_utf8_lossy(&gui_out.stdout),
            String::from_utf8_lossy(&gui_out.stderr)
        );
        assert!(
            cli_out.status.success(),
            "CLI registration worker failed (status {:?})\nstdout:\n{}\nstderr:\n{}",
            cli_out.status,
            String::from_utf8_lossy(&cli_out.stdout),
            String::from_utf8_lossy(&cli_out.stderr)
        );

        // ---- Phase 2: spawn-failing registration concurrent with a successful
        // one. The failing registration must reach launch_failed while the
        // successful native stub is still held.
        let ok2 = spawn(
            "ok2", "go2", &ok2_cwd, "OK2 Session", &stub_path, &ok2_logs.0, &ok2_logs.1,
            &ok2_logs.2, &ok2_logs.3,
        );
        fixture.track("ok2", &ok2_logs.3, ok2);
        let fail = spawn(
            "fail", "go2", &fail_cwd, "Fail Session", &fail_path, &fail_logs.0, &fail_logs.1,
            &fail_logs.2, &fail_logs.3,
        );
        fixture.track("fail", &fail_logs.3, fail);

        assert!(
            wait_for_file(&barrier.join("ready-ok2"), Duration::from_secs(30)),
            "ok2 worker never became ready"
        );
        assert!(
            wait_for_file(&barrier.join("ready-fail"), Duration::from_secs(30)),
            "fail worker never became ready"
        );
        fs::write(barrier.join("go2"), b"").expect("release phase 2 barrier");
        assert!(
            wait_for_file(&barrier.join("calling-ok2"), Duration::from_secs(30)),
            "ok2 worker never reached create_session"
        );
        assert!(
            wait_for_file(&barrier.join("calling-fail"), Duration::from_secs(30)),
            "fail worker never reached create_session"
        );
        let ok2_launched =
            wait_for_either(&ok2_logs.2, &barrier.join("error-ok2"), Duration::from_secs(30));
        let fail_observed = wait_for_either(
            &barrier.join("failed-fail"),
            &barrier.join("error-fail"),
            Duration::from_secs(30),
        );
        if !ok2_launched
            || !fail_observed
            || barrier.join("error-ok2").exists()
            || barrier.join("error-fail").exists()
        {
            panic_with_fixture_outputs(
                &mut fixture,
                format!(
                    "phase 2: failing registration must reach launch_failed while the successful native stub is held; ok2_launched={ok2_launched} fail_observed={fail_observed} ok2_error={:?} fail_error={:?}",
                    fs::read_to_string(barrier.join("error-ok2")).unwrap_or_default(),
                    fs::read_to_string(barrier.join("error-fail")).unwrap_or_default(),
                ),
            );
        }
        // Only after the failing rollback has been observed do we release the
        // still-running successful native stub.
        fixture.release_all();
        let ok2_out = fixture.take("ok2");
        let fail_out = fixture.take("fail");
        assert!(
            ok2_out.status.success(),
            "OK2 registration worker failed (status {:?})\nstdout:\n{}\nstderr:\n{}",
            ok2_out.status,
            String::from_utf8_lossy(&ok2_out.stdout),
            String::from_utf8_lossy(&ok2_out.stderr)
        );
        assert!(
            fail_out.status.success(),
            "spawn-failing worker did not observe launch_failed (status {:?})\nstdout:\n{}\nstderr:\n{}",
            fail_out.status,
            String::from_utf8_lossy(&fail_out.stdout),
            String::from_utf8_lossy(&fail_out.stderr)
        );
        assert!(
            barrier.join("failed-fail").exists(),
            "spawn-failing worker must have observed launch_failed"
        );

        // ---- Public canonical state: all successful registrations retained,
        // failed registration absent.
        let listed = sessions_list().expect("public sessions listing").data;
        let names: Vec<String> = listed
            .iter()
            .filter_map(|value| value["name"].as_str().map(str::to_string))
            .collect();
        for expected in ["GUI Session", "CLI Session", "OK2 Session"] {
            let entry = listed
                .iter()
                .find(|value| value["name"].as_str() == Some(expected))
                .unwrap_or_else(|| panic!("missing session {expected}; names={names:?}"));
            assert_eq!(entry["status"].as_str(), Some("pending_bind"));
            assert_eq!(entry["tool_session_id"].as_str(), Some(""));
        }
        assert!(
            !names.contains(&"Fail Session".to_string()),
            "failed spawn must not leave a recoverable record: {names:?}"
        );

        let store_path = StorageEngine::sessions_path().expect("sessions path");
        let raw = fs::read_to_string(&store_path).expect("read sessions store");
        let blob: EncryptedBlob =
            serde_json::from_str(&raw).expect("complete encrypted sessions blob");
        assert!(blob.is_encrypted, "sessions store must be encrypted");
        assert!(!blob.data.is_empty(), "encrypted payload must not be empty");
        let decrypted = load_sessions_state().expect("sessions store must decrypt");
        for expected in ["GUI Session", "CLI Session", "OK2 Session"] {
            assert!(
                decrypted.sessions.iter().any(|session| session.name == expected),
                "decrypted canonical state must contain {expected}"
            );
        }
        assert!(!decrypted.sessions.iter().any(|session| session.name == "Fail Session"));

        // Native adapter evidence: prepared argv and invoking cwd for workers.
        let gui_argv = fs::read_to_string(&gui_logs.0).expect("gui native argv");
        assert!(gui_argv.contains("--session-id"), "gui argv: {gui_argv:?}");
        assert!(fs::read_to_string(&gui_logs.1).expect("gui native pwd").contains("gui-cwd"));
        assert!(fs::read_to_string(&cli_logs.1).expect("cli native pwd").contains("cli-cwd"));
    });
}
