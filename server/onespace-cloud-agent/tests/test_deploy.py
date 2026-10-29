#!/usr/bin/env python3
"""Behavior tests for deploy.sh using fake SSH/SCP transport."""

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

SERVER_DIR = Path(__file__).resolve().parents[1]
DEPLOY = SERVER_DIR / "deploy.sh"
EXAMPLE = SERVER_DIR / "config.example.json"


class DeployHarness:
    def __init__(self, missing=None, fail_cmds=None, scp_fail=False):
        self.tmp = tempfile.mkdtemp()
        self.remote = Path(self.tmp) / "remote"
        self.install = self.remote / "install"
        self.systemd = self.remote / "systemd"
        self.bindir = self.remote / "bin"
        self.bashrc = self.remote / "bashrc"
        self.opencode = self.remote / "opencode" / "bin"
        self.remote_bin = self.remote / "remote_bin"
        for p in (self.install, self.systemd, self.bindir, self.opencode, self.remote_bin):
            p.mkdir(parents=True, exist_ok=True)
        self.local_bin = Path(self.tmp) / "localbin"
        self.local_bin.mkdir()
        self.missing = set(missing or [])
        self.fail_cmds = set(fail_cmds or [])
        self.scp_fail = scp_fail
        self.make_remote_bin()
        self.write_fake_ssh()
        self.write_fake_scp()

    def make_remote_bin(self):
        for name in ("python3", "git", "tmux", "gh", "opencode", "systemctl", "curl", "mv", "cp", "mkdir", "chmod", "ln", "grep", "sleep", "uname"):
            if name in self.missing:
                continue
            p = self.remote_bin / name
            if name in self.fail_cmds:
                body = "#!/usr/bin/env bash\nexit 1\n"
            elif name == "python3":
                body = "#!/usr/bin/env bash\nexec /usr/bin/python3 \"$@\"\n"
            elif name == "systemctl":
                body = "#!/usr/bin/env bash\nexit 0\n"
            elif name == "curl":
                body = "#!/usr/bin/env bash\necho '{\"status\":\"ok\"}'\nexit 0\n"
            else:
                real = shutil.which(name)
                if real:
                    body = "#!/usr/bin/env bash\nexec %s \"$@\"\n" % real
                else:
                    body = "#!/usr/bin/env bash\nexit 0\n"
            p.write_text(body)
            p.chmod(0o755)

    def write_fake_ssh(self):
        script = self.local_bin / "ssh"
        script.write_text("""#!/usr/bin/env python3
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

remote_root = os.environ["REMOTE_ROOT"]
install = os.path.join(remote_root, "install")
systemd = os.path.join(remote_root, "systemd")
bindir = os.path.join(remote_root, "bin")
bashrc = os.path.join(remote_root, "bashrc")
opencode = os.path.join(remote_root, "opencode", "bin")

for a, b in [("/root/onespace-cloud-agent", install),
             ("/etc/systemd/system", systemd),
             ("/usr/local/bin", bindir),
             ("/root/.bashrc", bashrc),
             ("/root/.opencode/bin", opencode)]:
    cmd = cmd.replace(a, b)

env = dict(os.environ)
env["PATH"] = os.path.join(remote_root, "remote_bin") + os.pathsep + env.get("PATH", "")
proc = subprocess.run(cmd, shell=True, env=env)
sys.exit(proc.returncode)
""")
        script.chmod(0o755)

    def write_fake_scp(self):
        script = self.local_bin / "scp"
        script.write_text("""#!/usr/bin/env python3
import os, sys, shutil
from pathlib import Path

args = [a for a in sys.argv[1:] if a not in ("-q",) and not a.startswith("-")]
if not args:
    sys.exit(0)
srcs = args[:-1]
dest_arg = args[-1]

remote_root = os.environ["REMOTE_ROOT"]
install = os.path.join(remote_root, "install")
systemd = os.path.join(remote_root, "systemd")
bindir = os.path.join(remote_root, "bin")

rpath = dest_arg
if ":" in rpath:
    rpath = rpath.split(":", 1)[1]
for a, b in [("/root/onespace-cloud-agent", install),
             ("/etc/systemd/system", systemd),
             ("/usr/local/bin", bindir)]:
    rpath = rpath.replace(a, b)

if os.environ.get("SCP_FAIL") == "1":
    sys.exit(1)

dest = Path(rpath)
if len(srcs) > 1 or dest.suffix == "":
    dest.mkdir(parents=True, exist_ok=True)
    for s in srcs:
        shutil.copy(s, dest / Path(s).name)
else:
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy(srcs[0], dest)
sys.exit(0)
""")
        script.chmod(0o755)

    def env(self):
        e = dict(os.environ)
        e["REMOTE_ROOT"] = str(self.remote)
        e["PATH"] = str(self.local_bin) + os.pathsep + e.get("PATH", "")
        if self.scp_fail:
            e["SCP_FAIL"] = "1"
        return e

    def run(self, extra_env=None, remote_config=None, existing_install=False, existing_config=None):
        if existing_install:
            self.install.mkdir(parents=True, exist_ok=True)
        if existing_config is not None:
            self.install.mkdir(parents=True, exist_ok=True)
            (self.install / "config.json").write_bytes(existing_config)
        if remote_config is not None:
            self.install.mkdir(parents=True, exist_ok=True)
            (self.install / "config.json").write_bytes(remote_config)
        e = self.env()
        if extra_env:
            e.update(extra_env)
        proc = subprocess.run(
            ["bash", str(DEPLOY), "fakehost"],
            cwd=str(SERVER_DIR),
            env=e,
            capture_output=True,
            text=True,
            timeout=60,
        )
        return proc

    def cleanup(self):
        shutil.rmtree(self.tmp, ignore_errors=True)


class TestDeploySyntax(unittest.TestCase):
    def test_bash_syntax(self):
        r = subprocess.run(["bash", "-n", str(DEPLOY)], capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)


class TestDeployValidation(unittest.TestCase):
    def run_case(self, **kw):
        h = DeployHarness(**kw)
        try:
            return h, h.run(**{k: v for k, v in kw.items() if k in ("remote_config", "existing_install", "existing_config")})
        finally:
            pass

    def test_invalid_remote_json_exits_nonzero(self):
        h = DeployHarness()
        try:
            bad = b"{not json"
            proc = h.run(existing_install=True, existing_config=bad)
            self.assertNotEqual(proc.returncode, 0)
        finally:
            h.cleanup()

    def test_missing_python3_exits_nonzero(self):
        h = DeployHarness(missing={"python3"})
        try:
            proc = h.run(existing_install=True, existing_config=b'{"a":1}')
            self.assertNotEqual(proc.returncode, 0)
        finally:
            h.cleanup()

    def test_missing_git_exits_nonzero(self):
        h = DeployHarness(missing={"git"})
        try:
            proc = h.run(existing_install=True, existing_config=b'{"a":1}')
            self.assertNotEqual(proc.returncode, 0)
        finally:
            h.cleanup()

    def test_missing_tmux_exits_nonzero(self):
        h = DeployHarness(missing={"tmux"})
        try:
            proc = h.run(existing_install=True, existing_config=b'{"a":1}')
            self.assertNotEqual(proc.returncode, 0)
        finally:
            h.cleanup()

    def test_missing_gh_exits_nonzero(self):
        h = DeployHarness(missing={"gh"})
        try:
            proc = h.run(existing_install=True, existing_config=b'{"a":1}')
            self.assertNotEqual(proc.returncode, 0)
        finally:
            h.cleanup()

    def test_missing_opencode_exits_nonzero(self):
        h = DeployHarness(missing={"opencode"})
        try:
            proc = h.run(existing_install=True, existing_config=b'{"a":1}')
            self.assertNotEqual(proc.returncode, 0)
        finally:
            h.cleanup()

    def test_upload_failure_exits_nonzero(self):
        h = DeployHarness(scp_fail=True)
        try:
            proc = h.run(existing_install=True, existing_config=b'{"a":1}')
            self.assertNotEqual(proc.returncode, 0)
        finally:
            h.cleanup()

    def test_first_deployment_no_false_backup(self):
        h = DeployHarness()
        try:
            proc = h.run()
            # count backup-like dirs outside install
            backups = [p for p in h.remote.iterdir() if p.is_dir() and p != h.install and p != h.systemd and p != h.bindir and p != h.remote_bin and "backup" in p.name.lower() or "snapshot" in p.name.lower()]
            self.assertEqual(len(backups), 0)
        finally:
            h.cleanup()


class TestDeploySuccess(unittest.TestCase):
    def test_first_deployment_initializes_config_from_example(self):
        h = DeployHarness()
        try:
            proc = h.run()
            self.assertEqual(proc.returncode, 0, proc.stderr)
            cfg = (h.install / "config.json").read_bytes()
            self.assertEqual(cfg, EXAMPLE.read_bytes())
        finally:
            h.cleanup()

    def test_repeated_deployment_preserves_config(self):
        h = DeployHarness()
        try:
            custom = b'{"custom": true}'
            proc = h.run(existing_install=True, existing_config=custom)
            self.assertEqual(proc.returncode, 0, proc.stderr)
            self.assertEqual((h.install / "config.json").read_bytes(), custom)
        finally:
            h.cleanup()

    def test_idempotent_rerun(self):
        h = DeployHarness()
        try:
            r1 = h.run()
            r2 = h.run()
            self.assertEqual(r1.returncode, 0)
            self.assertEqual(r2.returncode, 0)
        finally:
            h.cleanup()

    def test_launcher_names_and_yolo(self):
        h = DeployHarness()
        try:
            proc = h.run()
            self.assertEqual(proc.returncode, 0, proc.stderr)
            cs = (h.bindir / "cloud-session").read_text()
            oc = (h.bindir / "oc").read_text()
            self.assertIn("session_runner.py", cs)
            self.assertIn("--yolo", oc)
        finally:
            h.cleanup()

    def test_invalid_json_preserves_install(self):
        h = DeployHarness()
        try:
            before = [p.name for p in h.install.iterdir()] if h.install.exists() else []
            (h.install).mkdir(parents=True, exist_ok=True)
            (h.install / "marker.txt").write_text("active")
            (h.install / "config.json").write_bytes(b"{bad")
            proc = h.run()
            self.assertNotEqual(proc.returncode, 0)
            self.assertTrue((h.install / "marker.txt").exists())
        finally:
            h.cleanup()

    def test_promotion_failure_restores_old_dir(self):
        h = DeployHarness()
        try:
            (h.install).mkdir(parents=True, exist_ok=True)
            (h.install / "marker.txt").write_text("old")
            (h.install / "config.json").write_bytes(b'{"ok":1}')
            # force a failure during promotion by making mv fail
            h.fail_cmds.add("mv")
            h.make_remote_bin()
            proc = h.run()
            self.assertNotEqual(proc.returncode, 0)
            self.assertTrue((h.install / "marker.txt").exists())
        finally:
            h.cleanup()


if __name__ == "__main__":
    unittest.main()
