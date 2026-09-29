"""
@docs ARCHITECTURE:Infrastructure:Execution

### AI Assist Note
**Security Scan Test Harness**: Validates consolidated security scanner precision,
testing unquoted .env extraction, false-negative substring elimination, and OWASP rule enforcement.

### 🔍 Debugging & Observability
- **Failure Path**: Regression in secret pattern detection or unquoted .env scanning.
- **Telemetry Link**: Search `[test_security_scan]` in test execution logs.
"""

import os
import sys
import unittest
import tempfile
from pathlib import Path

# Add project root to sys.path
REPO_ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(REPO_ROOT))

from execution.security_scan import run_security_scan

class TestSecurityScan(unittest.TestCase):
    def setUp(self):
        self.test_dir = tempfile.TemporaryDirectory()
        self.root = Path(self.test_dir.name)

    def tearDown(self):
        self.test_dir.cleanup()

    def test_happy_path_clean_directory(self):
        """Clean project produces 0 findings and PASS status."""
        (self.root / "clean_module.py").write_text(
            "def calculate(a, b):\n    return a + b\n",
            encoding="utf-8"
        )
        res = run_security_scan(str(self.root))
        self.assertEqual(res["status"], "PASS")
        self.assertEqual(res["total_findings"], 0)

    def test_unquoted_env_secret_detected(self):
        """F1 regression: Unquoted secrets in .env files are detected."""
        env_file = self.root / ".env"
        env_file.write_text(
            "NEURAL_TOKEN=8f3a9c2b1d4e5f6a7b8c9d0e1f2a3b4c\n"
            "DB_PASSWORD=hunter2hunter2super\n",
            encoding="utf-8"
        )
        # Scan with scan_env=True for target fixture
        res = run_security_scan(str(self.root), scan_env=True)
        self.assertEqual(res["status"], "FAIL")
        self.assertGreaterEqual(res["high"], 2)
        types = [f["type"] for f in res["findings"]]
        self.assertTrue(any("NEURAL_TOKEN" in t for t in types))
        self.assertTrue(any("DB_PASSWORD" in t for t in types))

    def test_env_interpolation_not_flagged(self):
        """E1 edge case: Variable interpolation like ${VAR} is not treated as a leaked secret."""
        compose_file = self.root / "docker-compose.yml"
        compose_file.write_text(
            "version: '3.8'\n"
            "services:\n"
            "  app:\n"
            "    environment:\n"
            "      - NEURAL_TOKEN=${NEURAL_TOKEN:?required}\n"
            "      - DB_PASSWORD=${DB_PASSWORD}\n",
            encoding="utf-8"
        )
        res = run_security_scan(str(self.root))
        self.assertEqual(res["status"], "PASS")
        self.assertEqual(res["total_findings"], 0)

    def test_false_negative_substring_prevention(self):
        """Files with 'test' in name (e.g. latest.rs, greatest.py) are NOT skipped if not in test dirs."""
        prod_file = self.root / "latest.rs"
        prod_file.write_text(
            'pub const KEY: &str = "AKIA1234567890ABCDEF";\n',
            encoding="utf-8"
        )
        res = run_security_scan(str(self.root))
        self.assertEqual(res["status"], "FAIL")
        self.assertGreaterEqual(res["critical"], 1)
        self.assertTrue(any(f["file"] == "latest.rs" for f in res["findings"]))

    def test_bearer_prose_not_flagged(self):
        """E2 edge case: English prose mentioning 'bearer token' does not trigger critical alarm."""
        doc_file = self.root / "auth_guide.md"
        doc_file.write_text(
            "# Authentication\n\n"
            "Please use a bearer token to authenticate your requests.\n"
            "All bearer tokens must be transmitted over HTTPS.\n",
            encoding="utf-8"
        )
        res = run_security_scan(str(self.root))
        self.assertEqual(res["status"], "PASS")
        self.assertEqual(res["critical"], 0)

    def test_rust_cfg_test_module_ignored(self):
        """Secrets inside #[cfg(test)] modules in Rust code are permitted for test fixtures."""
        rust_file = self.root / "client.rs"
        rust_file.write_text(
            'pub fn connect() {}\n\n'
            '#[cfg(test)]\n'
            'mod tests {\n'
            '    const TEST_KEY: &str = "AKIAIOSFODNN7EXAMPLE";\n'
            '}\n',
            encoding="utf-8"
        )
        res = run_security_scan(str(self.root))
        self.assertEqual(res["status"], "PASS")
        self.assertEqual(res["total_findings"], 0)

    def test_nosec_annotation_respected(self):
        """E4 edge case: Lines with nosec annotations are ignored."""
        script_file = self.root / "util.py"
        script_file.write_text(
            '# nosec: intentional test credential for offline validation\n'
            'API_KEY = "sk-ant-api03-12345678901234567890"  # nosec\n',
            encoding="utf-8"
        )
        res = run_security_scan(str(self.root))
        self.assertEqual(res["status"], "PASS")
        self.assertEqual(res["total_findings"], 0)

if __name__ == "__main__":
    unittest.main()
