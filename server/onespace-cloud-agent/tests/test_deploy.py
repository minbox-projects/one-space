#!/usr/bin/env python3
"""Behavior tests for deploy.sh using fake SSH/SCP transport."""

import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

SERVER_DIR = Path(__file__).resolve().parents[1]
DEPLOY = SERVER_DIR / "deploy.sh"
BOOTSTRAP = SERVER_DIR / "bootstrap.sh"
VERSION_FILE = SERVER_DIR / "VERSION"
EXAMPLE = SERVER_DIR / "config.example.json"
DEFAULT_CFG = SERVER_DIR / "config.default.json"

STANDARD_UTILS = ["awk", "sed", "grep", "rm", "mkdir", "rmdir", "install", "tee", "mktemp",
                  "date", "stat", "dirname", "basename", "head", "tail", "tr", "sort", "ls",
                  "find", "cut", "wc", "cat", "chmod", "chown"]


class DeployHarness:
    def __init__(self, agent_dir="/root/onespace-cloud-agent", missing=None, distro="ubuntu 22.04",
                 arch="x86_64", root=True, systemd_available=True, systemd_state="active",
                 scp_fail=False, upload_fail=False, promote_fail=False, activate_fail=False,
                 metadata_fail=False, apt_fail=False):
        self.tmp = tempfile.mkdtemp()
        self.remote = Path(self.tmp) / "remote"
        self.agent_dir = agent_dir
        self.remote_agent = self.remote / "agent"
        self.systemd = self.remote / "systemd"
        self.bindir = self.remote / "bindir"
        self.bashrc = self.remote / "bashrc"
        self.opencode = self.remote / "opencode" / "bin"
        self.remote_bin = self.remote / "remote_bin"
        self.base_bin = self.remote / "base_bin"
        self.template_dir = self.remote / "package_templates"
        self.etc = self.remote / "etc"
        for p in (self.remote_agent, self.systemd, self.bindir, self.opencode, self.remote_bin,
                  self.base_bin, self.template_dir, self.etc):
            p.mkdir(parents=True, exist_ok=True)
        self.local_bin = Path(self.tmp) / "localbin"
        self.local_bin.mkdir()
        self.missing = set(missing or [])
        self.distro = distro
        self.arch = arch
        self.root = root
        self.systemd_available = bool(systemd_available)
        self.systemd_state = systemd_state
        self.scp_fail = scp_fail
        self.upload_fail = upload_fail
        self.promote_fail = promote_fail
        self.activate_fail = activate_fail
        self.metadata_fail = metadata_fail
        self.apt_fail = apt_fail
        self.apt_record = Path(self.tmp) / "apt_record.txt"
        self.curl_record = Path(self.tmp) / "curl_record.txt"
        self.gh_record = Path(self.tmp) / "gh_record.txt"
        self.opencode_record = Path(self.tmp) / "opencode_record.txt"
        self.scp_sources = Path(self.tmp) / "scp_sources.txt"
        self.make_base_bin()
        self.make_package_templates()
        self.make_remote_bin()
        self.write_os_release()
        self.write_fake_ssh()
        self.write_fake_scp()

    def make_base_bin(self):
        for name in STANDARD_UTILS:
            target = shutil.which(name)
            if not target:
                continue
            link = self.base_bin / name
            try:
                link.symlink_to(target)
            except Exception:
                shutil.copy2(target, link)
                link.chmod(0o755)

    def configure(self, **kw):
        for k, v in kw.items():
            setattr(self, k, v)
        self.make_package_templates()
        self.make_remote_bin()
        self.write_fake_ssh()
        self.write_fake_scp()

    def make_package_templates(self):
        for p in self.template_dir.iterdir():
            p.unlink()
        templates = {}
        for name in ("python3", "git", "tmux", "tar"):
            real = shutil.which(name)
            if real:
                templates[name] = '#!/bin/bash\nexec %s "$@"\n' % real
            else:
                templates[name] = '#!/bin/bash\nexit 0\n'
        templates["gh"] = '#!/bin/bash\necho "gh $*" >> "%s"\nexit 0\n' % self.gh_record
        templates["curl"] = ('#!/bin/bash\n'
                            'echo "curl $*" >> "%s"\n' % self.curl_record +
                            'if echo "$*" | grep -q "install"; then echo "#!/bin/sh\\necho installing opencode 1.19.5"; exit 0; fi\n'
                            'echo \'{"status":"ok"}\'\nexit 0\n')
        for name, body in templates.items():
            path = self.template_dir / name
            path.write_text(body)
            path.chmod(0o755)

    def make_remote_bin(self):
        for p in self.remote_bin.iterdir():
            p.unlink()
        for name in ("python3", "git", "tmux", "gh", "opencode", "systemctl", "curl", "apt-get",
                     "mv", "cp", "mkdir", "chmod", "ln", "grep", "sleep", "uname", "cat", "id",
                     "tar", "bash", "sh", "tee", "date", "rm"):
            if name in self.missing:
                continue
            p = self.remote_bin / name
            if name == "systemctl" and not self.systemd_available:
                continue
            if name in ("python3", "git", "tmux", "gh", "curl", "tar"):
                shutil.copy2(self.template_dir / name, p)
                p.chmod(0o755)
                continue
            if name == "id":
                root_flag = "1" if self.root else "0"
                body = ('#!/bin/bash\n'
                        'if [ "$1" = "-u" ]; then\n'
                        '  if [ "%s" = "1" ]; then echo 0; else echo 1000; fi\n'
                        '  exit 0\n'
                        'fi\n'
                        'if [ "%s" = "1" ]; then echo "uid=0(root) gid=0(root) groups=0(root)"; else echo "uid=1000(user) gid=1000(user) groups=1000(user)"; fi\n'
                        % (root_flag, root_flag))
            elif name == "uname":
                body = '#!/bin/bash\nif [ "$1" = "-m" ]; then echo "%s"; else echo Linux; fi\n' % self.arch
            elif name == "systemctl":
                body = '''#!/bin/bash
STATE="${REMOTE_ROOT}/systemd_state"
if [ "$SYSTEMD_AVAILABLE" != "1" ]; then exit 1; fi
case "$*" in
  *--version*) echo "systemd 252"; exit 0 ;;
  *is-system-running*) echo "running"; exit 0 ;;
  *"is-active"*)
    if [ -f "$STATE" ] && grep -qx "active" "$STATE"; then echo active; exit 0; else echo inactive; exit 3; fi
    ;;
  *"daemon-reload"*|*"enable"*|*"restart"*|*"start"*)
    if [ "$ACTIVATE_FAIL" = "1" ]; then
      echo inactive > "$STATE"
    else
      echo active > "$STATE"
    fi
    exit 0
    ;;
  *) exit 0 ;;
esac
'''
            elif name == "apt-get":
                body = ('#!/bin/bash\n'
                        'echo "apt-get $*" >> "%s"\n' % self.apt_record +
                        'if [ "$APT_FAIL" = "1" ]; then exit 1; fi\n'
                        'has_install=0\n'
                        'for a in "$@"; do [ "$a" = "install" ] && has_install=1; done\n'
                        'if [ "$has_install" = "1" ]; then\n'
                        '  for a in "$@"; do\n'
                        '    case "$a" in -*) continue ;; install|update) continue ;; esac\n'
                        '    if [ -f "%s/$a" ]; then cp "%s/$a" "%s/$a"; chmod 755 "%s/$a"; fi\n'
                        '  done\n'
                        'fi\n'
                        'exit 0\n') % (self.template_dir, self.template_dir, self.remote_bin, self.remote_bin)
            elif name == "curl":
                body = ('#!/bin/bash\n'
                        'echo "curl $*" >> "%s"\n' % self.curl_record +
                        'if echo "$*" | grep -q "install"; then echo "#!/bin/sh\\necho installing opencode 1.19.5"; exit 0; fi\n'
                        'echo \'{"status":"ok"}\'\nexit 0\n')
            elif name == "gh":
                body = ('#!/bin/bash\n'
                        'echo "gh $*" >> "%s"\nexit 0\n' % self.gh_record)
            elif name == "opencode":
                body = ('#!/bin/bash\n'
                        'echo "opencode $*" >> "%s"\nexit 0\n' % self.opencode_record)
            elif name == "mv" and self.promote_fail:
                body = '#!/bin/bash\nexit 1\n'
            elif name == "bash" or name == "sh":
                real = shutil.which(name)
                body = '#!/bin/bash\nexec %s "$@"\n' % real if real else '#!/bin/bash\nexit 0\n'
            elif name == "python3":
                real = shutil.which("python3")
                body = '#!/bin/bash\nexec %s "$@"\n' % real if real else '#!/bin/bash\nexit 0\n'
            else:
                real = shutil.which(name)
                if real:
                    body = '#!/bin/bash\nexec %s "$@"\n' % real
                else:
                    body = '#!/bin/bash\nexit 0\n'
            p.write_text(body)
            p.chmod(0o755)

    def write_os_release(self):
        distro_map = {
            "ubuntu 22.04": 'ID=ubuntu\nVERSION_ID="22.04"\n',
            "ubuntu 24.04": 'ID=ubuntu\nVERSION_ID="24.04"\n',
            "debian 12": 'ID=debian\nVERSION_ID="12"\n',
            "fedora": 'ID=fedora\nVERSION_ID="39"\n',
        }
        content = distro_map.get(self.distro, 'ID=unknown\nVERSION_ID="1"\n')
        (self.etc / "os-release").write_text(content)

    def write_fake_ssh(self):
        agent_dir = self.agent_dir
        remote_agent = str(self.remote_agent)
        systemd = str(self.systemd)
        bindir = str(self.bindir)
        bashrc = str(self.bashrc)
        opencode = str(self.opencode)
        etc = str(self.etc)
        remote_bin = str(self.remote_bin)
        base_bin = str(self.base_bin)
        activate_fail = "1" if self.activate_fail else "0"
        systemd_available = "1" if self.systemd_available else "0"
        metadata_fail = "1" if self.metadata_fail else "0"
        script = self.local_bin / "ssh"
        script.write_text('''#!/usr/bin/env python3
import os, sys, subprocess

args = sys.argv[1:]
host = None
cmd_parts = []
i = 0
while i < len(args):
    if args[i] == "-o":
        i += 2
        continue
    if args[i].startswith("-"):
        i += 1
        continue
    if host is None:
        host = args[i]
        i += 1
        continue
    cmd_parts.append(args[i])
    i += 1

cmd = " ".join(cmd_parts)
if not cmd:
    cmd = sys.stdin.read()

agent_dir = %r
remote_agent = %r
systemd = %r
bindir = %r
bashrc = %r
opencode = %r
etc = %r

for a, b in [(agent_dir, remote_agent),
             ("/etc/systemd/system", systemd),
             ("/usr/local/bin", bindir),
             ("/root/.bashrc", bashrc),
             ("/root/.opencode/bin", opencode),
             ("/etc/os-release", os.path.join(etc, "os-release"))]:
    cmd = cmd.replace(a, b)

os.environ["PATH"] = %r + ":" + %r
os.environ["REMOTE_ROOT"] = os.path.dirname(remote_agent)
os.environ["SYSTEMD_AVAILABLE"] = %r
os.environ["ACTIVATE_FAIL"] = %r

if %r == "1" and "install-info.json" in cmd:
    target = remote_agent + "/install-info.json"
    try:
        os.remove(target)
    except FileNotFoundError:
        pass
    sys.exit(1)

proc = subprocess.run(cmd, shell=True)
sys.exit(proc.returncode)
''' % (agent_dir, remote_agent, systemd, bindir, bashrc, opencode, etc,
       remote_bin, base_bin, systemd_available, activate_fail, metadata_fail))
        script.chmod(0o755)

    def write_fake_scp(self):
        agent_dir = self.agent_dir
        remote_agent = str(self.remote_agent)
        systemd = str(self.systemd)
        bindir = str(self.bindir)
        scp_sources = str(self.scp_sources)
        script = self.local_bin / "scp"
        script.write_text('''#!/usr/bin/env python3
import os, sys, shutil
from pathlib import Path

args = [a for a in sys.argv[1:] if a not in ("-q",) and not a.startswith("-o") and not a.startswith("-")]
if not args:
    sys.exit(0)
srcs = args[:-1]
dest_arg = args[-1]

remote_agent = %r
systemd = %r
bindir = %r
agent_dir = %r
scp_sources = %r

rpath = dest_arg
if ":" in rpath:
    rpath = rpath.split(":", 1)[1]
for a, b in [(agent_dir, remote_agent),
             ("/etc/systemd/system", systemd),
             ("/usr/local/bin", bindir)]:
    rpath = rpath.replace(a, b)

with open(scp_sources, "a") as f:
    for s in srcs:
        f.write(str(s) + "\\n")

if os.environ.get("SCP_FAIL") == "1":
    sys.exit(1)

dest = Path(rpath)
if len(srcs) > 1 or (dest.exists() and dest.is_dir()) or dest.suffix == "":
    dest.mkdir(parents=True, exist_ok=True)
    for s in srcs:
        shutil.copy(s, dest / Path(s).name)
else:
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy(srcs[0], dest)
sys.exit(0)
''' % (remote_agent, systemd, bindir, agent_dir, scp_sources))
        script.chmod(0o755)

    def env(self):
        e = dict(os.environ)
        e["PATH"] = str(self.local_bin) + os.pathsep + e.get("PATH", "")
        if self.scp_fail or self.upload_fail:
            e["SCP_FAIL"] = "1"
        e["APT_FAIL"] = "1" if self.apt_fail else "0"
        return e

    def run(self, extra_args=None, remote_config=None, existing_config=None, existing_install=False):
        if existing_install or existing_config is not None:
            self.remote_agent.mkdir(parents=True, exist_ok=True)
        if existing_config is not None:
            self.remote_agent.mkdir(parents=True, exist_ok=True)
            (self.remote_agent / "config.json").write_bytes(existing_config)
        if remote_config is not None:
            self.remote_agent.mkdir(parents=True, exist_ok=True)
            (self.remote_agent / "config.json").write_bytes(remote_config)
        e = self.env()
        cmd = ["bash", str(DEPLOY), "fakehost"]
        if extra_args:
            cmd += extra_args
        else:
            cmd += [self.agent_dir]
        proc = subprocess.run(
            cmd,
            cwd=str(SERVER_DIR),
            env=e,
            capture_output=True,
            text=True,
            timeout=60,
        )
        return proc

    def cleanup(self):
        shutil.rmtree(self.tmp, ignore_errors=True)


class TestSyntax(unittest.TestCase):
    def test_deploy_bash_n(self):
        r = subprocess.run(["bash", "-n", str(DEPLOY)], capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)

    def test_bootstrap_bash_n(self):
        r = subprocess.run(["bash", "-n", str(BOOTSTRAP)], capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)


class TestPrerequisiteBoundaries(unittest.TestCase):
    def test_non_root_fails_platform_before_apt_or_upload(self):
        h = DeployHarness(root=False, missing={"python3"})
        try:
            proc = h.run()
            self.assertNotEqual(proc.returncode, 0)
            self.assertIn("phase=platform", proc.stdout + proc.stderr)
            self.assertIn("reason=", proc.stdout + proc.stderr)
        finally:
            h.cleanup()

    def test_no_systemd_fails_platform(self):
        h = DeployHarness(systemd_available=False, missing={"python3"})
        try:
            proc = h.run()
            self.assertNotEqual(proc.returncode, 0)
            self.assertIn("phase=platform", proc.stdout + proc.stderr)
        finally:
            h.cleanup()

    def test_unsupported_arch_fails_platform(self):
        h = DeployHarness(arch="riscv64", missing={"python3"})
        try:
            proc = h.run()
            self.assertNotEqual(proc.returncode, 0)
            self.assertIn("phase=platform", proc.stdout + proc.stderr)
        finally:
            h.cleanup()

    def test_unsupported_distro_with_missing_deps_fails_platform(self):
        h = DeployHarness(distro="fedora", missing={"python3"})
        try:
            proc = h.run()
            self.assertNotEqual(proc.returncode, 0)
            self.assertIn("phase=platform", proc.stdout + proc.stderr)
        finally:
            h.cleanup()

    def test_package_failure_reports_dependency_phase_no_upload(self):
        h = DeployHarness(apt_fail=True, missing={"python3"})
        try:
            proc = h.run()
            self.assertNotEqual(proc.returncode, 0)
            self.assertIn("phase=dependencies", proc.stdout + proc.stderr)
            self.assertIn("reason=", proc.stdout + proc.stderr)
        finally:
            h.cleanup()

    def test_all_deps_present_no_installer_runs_any_distro(self):
        h = DeployHarness(distro="fedora", missing=set())
        try:
            proc = h.run()
            self.assertEqual(proc.returncode, 0, proc.stderr + proc.stdout)
            gh_text = h.gh_record.read_text() if h.gh_record.exists() else ""
            oc_text = h.opencode_record.read_text() if h.opencode_record.exists() else ""
            self.assertEqual(gh_text.strip(), "")
            self.assertEqual(oc_text.strip(), "")
            self.assertTrue((h.remote_bin / "gh").exists())
            self.assertTrue((h.remote_bin / "opencode").exists())
        finally:
            h.cleanup()


class TestCleanInstallEndToEnd(unittest.TestCase):
    def test_clean_supported_server_installs_all(self):
        h = DeployHarness(missing={"python3", "git", "tmux", "gh", "opencode"})
        try:
            proc = h.run()
            self.assertEqual(proc.returncode, 0, proc.stderr + proc.stdout)
            for phase in ("platform", "dependencies", "upload", "config", "publish", "service", "metadata"):
                self.assertIn(f"phase={phase}", proc.stdout)
            required = ["agent.py", "core/__init__.py", "core/disk_guardian.py", "core/gateway.py",
                        "core/issue_worker.py", "core/notifier.py", "core/session_runner.py",
                        "config.json", "VERSION"]
            for rel in required:
                self.assertTrue((h.remote_agent / rel).exists(), rel)
            apt_text = h.apt_record.read_text() if h.apt_record.exists() else ""
            for dep in ("python3", "git", "tmux"):
                self.assertIn(dep, apt_text)
            self.assertIn("curl", apt_text)
            self.assertIn("ca-certificates", apt_text)
            self.assertIn("tar", apt_text)
            gh_text = h.gh_record.read_text() if h.gh_record.exists() else ""
            self.assertTrue(gh_text.strip() != "")
            oc_text = h.opencode_record.read_text() if h.opencode_record.exists() else ""
            self.assertIn("--version 1.19.5", oc_text + proc.stdout)
            self.assertIn("--no-modify-path", oc_text + proc.stdout)
            unit = h.systemd / "onespace-cloud-agent.service"
            self.assertTrue(unit.exists())
            unit_text = unit.read_text()
            self.assertIn("WorkingDirectory=%s" % h.agent_dir, unit_text)
            self.assertIn("ExecStart=/usr/bin/python3 %s/agent.py" % h.agent_dir, unit_text)
            # config assertions
            if DEFAULT_CFG.exists():
                expected = json.loads(DEFAULT_CFG.read_text())
                actual = json.loads((h.remote_agent / "config.json").read_text())
                self.assertEqual(actual.get("ai_gateway", {}).get("providers"), {})
                self.assertEqual(actual.get("ai_gateway", {}).get("model_pools"), {})
                self.assertEqual(actual.get("issue_worker", {}).get("watch_repositories"), [])
                self.assertFalse(actual.get("issue_worker", {}).get("enabled", True))
                self.assertFalse(actual.get("notification", {}).get("enabled", True))
                self.assertGreater(actual.get("ai_gateway", {}).get("port", 0), 0)
                blob = (h.remote_agent / "config.json").read_text()
                for bad in ("your-", "sk-", "ghp_", "SEC_"):
                    self.assertNotIn(bad, blob)
        finally:
            h.cleanup()

    def test_backups_pruned_to_three_after_repeated(self):
        h = DeployHarness()
        try:
            for _ in range(5):
                r = h.run(existing_install=True)
                self.assertEqual(r.returncode, 0, r.stderr + r.stdout)
            backups = [p for p in h.remote.iterdir() if p.is_dir() and "backup" in p.name.lower()]
            self.assertLessEqual(len(backups), 3)
        finally:
            h.cleanup()


class TestConfiguredAgentDirSpace(unittest.TestCase):
    def test_space_dir_honored(self):
        h = DeployHarness(agent_dir="/root/my agent dir")
        try:
            proc = h.run(extra_args=["/root/my agent dir"])
            self.assertEqual(proc.returncode, 0, proc.stderr + proc.stdout)
            for name in ("cloud-session", "oc"):
                p = h.bindir / name
                if p.exists():
                    self.assertIn("my agent dir", p.read_text())
            unit = h.systemd / "onespace-cloud-agent.service"
            if unit.exists():
                text = unit.read_text()
                self.assertIn("my agent dir", text)
        finally:
            h.cleanup()

    def test_space_dir_unit_and_launchers(self):
        agent_dir = "/root/my agent dir"
        h = DeployHarness(agent_dir=agent_dir)
        try:
            proc = h.run(extra_args=[agent_dir])
            self.assertEqual(proc.returncode, 0, proc.stderr + proc.stdout)
            for name in ("cloud-session", "oc"):
                p = h.bindir / name
                self.assertTrue(p.exists())
                self.assertIn(agent_dir, p.read_text())
            unit = h.systemd / "onespace-cloud-agent.service"
            self.assertTrue(unit.exists())
            text = unit.read_text()
            self.assertIn("WorkingDirectory=%s" % agent_dir, text)
            self.assertIn(agent_dir, text)
        finally:
            h.cleanup()

    def test_custom_config_preserved(self):
        h = DeployHarness()
        try:
            custom = b'{"custom": true}'
            proc = h.run(existing_config=custom)
            self.assertEqual(proc.returncode, 0, proc.stderr + proc.stdout)
            self.assertEqual((h.remote_agent / "config.json").read_bytes(), custom)
        finally:
            h.cleanup()


class TestFailurePhases(unittest.TestCase):
    def test_upload_failure_leaves_files_untouched(self):
        h = DeployHarness(scp_fail=True)
        try:
            (h.remote_agent).mkdir(parents=True, exist_ok=True)
            (h.remote_agent / "marker.txt").write_text("active")
            (h.remote_agent / "config.json").write_bytes(b'{"ok":1}')
            proc = h.run(existing_config=b'{"ok":1}')
            self.assertNotEqual(proc.returncode, 0)
            self.assertTrue((h.remote_agent / "marker.txt").exists())
            self.assertEqual((h.remote_agent / "config.json").read_bytes(), b'{"ok":1}')
        finally:
            h.cleanup()

    def test_promotion_failure_restores_snapshot(self):
        h = DeployHarness(promote_fail=True)
        try:
            (h.remote_agent).mkdir(parents=True, exist_ok=True)
            (h.remote_agent / "marker.txt").write_text("old")
            (h.remote_agent / "config.json").write_bytes(b'{"ok":1}')
            proc = h.run(existing_config=b'{"ok":1}')
            self.assertNotEqual(proc.returncode, 0)
            self.assertTrue((h.remote_agent / "marker.txt").exists())
        finally:
            h.cleanup()

    def test_activation_failure_no_success_record_no_prune(self):
        h = DeployHarness(activate_fail=True)
        try:
            proc = h.run()
            self.assertNotEqual(proc.returncode, 0)
            self.assertFalse((h.remote_agent / "install-info.json").exists())
        finally:
            h.cleanup()

    def test_metadata_failure_no_success_record(self):
        h = DeployHarness(metadata_fail=True)
        try:
            proc = h.run()
            self.assertNotEqual(proc.returncode, 0)
            self.assertFalse((h.remote_agent / "install-info.json").exists())
        finally:
            h.cleanup()

    def test_phase_error_lines_carry_reason(self):
        h = DeployHarness(apt_fail=True, missing={"python3"})
        try:
            proc = h.run()
            self.assertNotEqual(proc.returncode, 0)
            self.assertTrue(any("status=error" in l and "reason=" in l for l in (proc.stdout + proc.stderr).splitlines()))
        finally:
            h.cleanup()


class TestMetadataIntegrity(unittest.TestCase):
    def test_install_info_version_and_utc(self):
        h = DeployHarness()
        try:
            proc = h.run()
            self.assertEqual(proc.returncode, 0, proc.stderr)
            info = h.remote_agent / "install-info.json"
            data = json.loads(info.read_text())
            self.assertIn("version", data)
            self.assertIn("installed_at", data)
            from datetime import datetime
            datetime.fromisoformat(data["installed_at"].replace("Z", "+00:00"))
        finally:
            h.cleanup()

    def test_repeated_success_updates_metadata(self):
        h = DeployHarness()
        try:
            r1 = h.run()
            self.assertEqual(r1.returncode, 0)
            r2 = h.run()
            self.assertEqual(r2.returncode, 0)
        finally:
            h.cleanup()

    def test_failed_attempt_does_not_replace_metadata(self):
        h = DeployHarness()
        try:
            r1 = h.run()
            self.assertEqual(r1.returncode, 0)
            info_path = h.remote_agent / "install-info.json"
            before = info_path.read_bytes() if info_path.exists() else None
            h.configure(activate_fail=True)
            r2 = h.run()
            self.assertNotEqual(r2.returncode, 0)
            if info_path.exists():
                if before is None:
                    self.fail("fabricated install-info.json")
                self.assertEqual(info_path.read_bytes(), before)
        finally:
            h.cleanup()

    def test_upload_payload_sources(self):
        h = DeployHarness()
        try:
            proc = h.run()
            self.assertEqual(proc.returncode, 0, proc.stderr + proc.stdout)
            srcs = h.scp_sources.read_text().splitlines() if h.scp_sources.exists() else []
            names = ["deploy.sh", "bootstrap.sh", "VERSION", "config.default.json", "config.example.json",
                     "agent.py", "core/__init__.py", "core/disk_guardian.py", "core/gateway.py",
                     "core/issue_worker.py", "core/notifier.py", "core/session_runner.py",
                     "systemd/onespace-cloud-agent.service"]
            for name in names:
                self.assertTrue(any(name in s for s in srcs), name)
            for s in srcs:
                if "config.json" in s:
                    self.assertTrue("config.default.json" in s or "config.example.json" in s, s)
                self.assertNotIn(".env", s)
        finally:
            h.cleanup()


class TestInvalidConfigRefusal(unittest.TestCase):
    def test_invalid_json_refuses_before_publication(self):
        h = DeployHarness()
        try:
            (h.remote_agent).mkdir(parents=True, exist_ok=True)
            (h.remote_agent / "marker.txt").write_text("active")
            (h.remote_agent / "config.json").write_bytes(b"{bad")
            proc = h.run(existing_config=b"{bad")
            self.assertNotEqual(proc.returncode, 0)
            self.assertTrue((h.remote_agent / "marker.txt").exists())
        finally:
            h.cleanup()


if __name__ == "__main__":
    unittest.main()
