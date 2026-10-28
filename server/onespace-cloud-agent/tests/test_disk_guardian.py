#!/usr/bin/env python3
"""
disk_guardian.py 单元测试
"""

import os
import tempfile
import unittest
from core.disk_guardian import DiskGuardian


class TestDiskGuardian(unittest.TestCase):
    def setUp(self):
        self.tmp_dir = tempfile.mkdtemp(prefix="test_disk_guardian_")
        self.guardian = DiskGuardian(opencode_dir=self.tmp_dir, min_free_gb=1.0)

    def tearDown(self):
        import shutil
        if os.path.exists(self.tmp_dir):
            shutil.rmtree(self.tmp_dir, ignore_errors=True)

    def test_disk_usage(self):
        usage = self.guardian.get_disk_usage("/")
        self.assertIn("total_gb", usage)
        self.assertIn("free_gb", usage)
        self.assertGreater(usage["total_gb"], 0)

    def test_cleanup_workspace(self):
        dummy_ws = os.path.join(self.tmp_dir, "ws1")
        os.makedirs(dummy_ws, exist_ok=True)
        with open(os.path.join(dummy_ws, "test.txt"), "w") as f:
            f.write("hello")
        self.assertTrue(os.path.exists(dummy_ws))
        self.guardian.cleanup_workspace(dummy_ws)
        self.assertFalse(os.path.exists(dummy_ws))


if __name__ == "__main__":
    unittest.main()
