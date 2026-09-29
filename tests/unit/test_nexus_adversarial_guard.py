"""
@docs ARCHITECTURE:Core

### AI Assist Note
**Unit Tests: Nexus Adversarial Guard**
Tests each adversarial invariant check against synthetic fixtures.

### 🔍 Debugging & Observability
- **Failure Path**: Invariant check evasion or synthetic fixture misfire.
- **Telemetry Link**: Search `[nexus_adversarial_guard]` in test logs.
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "execution"))
import nexus_adversarial_guard as guard

C5_ADVERSARIAL_FIXTURE = """\
fn authorize(payload: &Payload, role: &str, claimed_id: &str) -> bool {
    let is_authorized_operator = matches!(role, "admin")
        && payload.user_id == claimed_id
        && payload.auto_resume == false;
    is_authorized_operator
}
"""

C1_CLOBBER_FIXTURE = """\
pub async fn execute_save_agent(&self, agent: &EngineAgent) -> Result<(), AppError> {
    reconciled.version = db_ver;
    Ok(())
}
"""

C2_BAD_CASCADE_FIXTURE = """\
pub async fn delete_agent_cascade(&self, agent_id: &str) -> Result<(), AppError> {
    sqlx::query("DELETE FROM agents WHERE id = ?").execute(&self.pool).await?;
    sqlx::query("DELETE FROM mission_history WHERE agent_id = ?").execute(&self.pool).await?;
    sqlx::query("DELETE FROM mission_logs WHERE agent_id = ?").execute(&self.pool).await?;
    Ok(())
}
"""

C2_GOOD_CASCADE_FIXTURE = """\
pub async fn delete_agent_cascade(&self, agent_id: &str) -> Result<(), AppError> {
    sqlx::query("DELETE FROM mission_logs WHERE agent_id = ?").execute(&self.pool).await?;
    sqlx::query("DELETE FROM mission_history WHERE agent_id = ?").execute(&self.pool).await?;
    sqlx::query("DELETE FROM agents WHERE id = ?").execute(&self.pool).await?;
    Ok(())
}
"""


class TestNexusAdversarialGuard(unittest.TestCase):
    def test_c1_detects_version_clobber(self):
        errors = guard.check_c1_version_clobber(C1_CLOBBER_FIXTURE)
        self.assertTrue(any("C1-FAIL" in e for e in errors))

    def test_c1_unverified_when_save_agent_missing(self):
        errors = guard.check_c1_version_clobber("fn unrelated() {}")
        self.assertTrue(any("C1-UNVERIFIED" in e for e in errors))

    def test_c2_detects_inverted_cascade_deletion(self):
        errors = guard.check_c2_cascade_ordering(C2_BAD_CASCADE_FIXTURE)
        self.assertTrue(any("C2-FAIL" in e for e in errors))

    def test_c2_passes_correct_cascade_deletion(self):
        errors = guard.check_c2_cascade_ordering(C2_GOOD_CASCADE_FIXTURE)
        self.assertEqual(errors, [])

    def test_c2_unverified_when_target_missing(self):
        errors = guard.check_c2_cascade_ordering("fn unrelated() {}")
        self.assertTrue(any("C2-UNVERIFIED" in e for e in errors))

    def test_c5_detects_multiline_auth_bypass(self):
        errors = guard.check_c5_untrusted_auth_bypass(C5_ADVERSARIAL_FIXTURE)
        self.assertTrue(any("C5-FAIL" in e for e in errors))

    def test_c6_unverified_when_list_graph_missing(self):
        errors = guard.check_c6_graph_path_validation("fn unrelated() {}")
        self.assertTrue(any("C6-UNVERIFIED" in e for e in errors))

    def test_h1_unverified_when_update_mission_missing(self):
        errors = guard.check_h1_mission_validation("fn unrelated() {}")
        self.assertTrue(any("H1-UNVERIFIED" in e for e in errors))


if __name__ == "__main__":
    unittest.main()
