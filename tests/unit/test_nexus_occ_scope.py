"""
@docs ARCHITECTURE:TestSuites
### AI Assist Note
Tests that the OCC audit gate detects a clobber within a function without
combining unrelated Rust function bodies.
### 🔍 Debugging & Observability
- Failure Path: False positive across functions or missed version-adopting retry.
- Telemetry Link: Python unittest assertions.
"""
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "execution"))
from nexus_adversarial_guard import check_c3_retry_occ_clobber


class NexusOccScopeTests(unittest.TestCase):
    def test_detects_version_adopting_retry_in_same_function(self):
        code = """pub async fn save() {
    if let Err(AppError::Conflict(_)) = result {
        agent.version = current_version + 1;
        retry().await;
    }
}
"""
        self.assertTrue(check_c3_retry_occ_clobber(code))

    def test_detects_clobber_in_indented_impl_function(self):
        code = """impl Store {
    pub async fn save(&self) {
        if let Err(AppError::Conflict(_)) = result {
            agent.version = latest + 1;
            retry().await;
        }
    }
}
"""
        self.assertTrue(check_c3_retry_occ_clobber(code))

    def test_does_not_combine_unrelated_function_bodies(self):
        code = """pub async fn conflict() {
    return Err(AppError::Conflict("stale".into()));
}
async fn fixture() {
    agent.version = save_agent().await;
    assert_eq!(agent.version, old_version + 1);
    retry_assertions();
}
"""
        self.assertEqual(check_c3_retry_occ_clobber(code), [])


if __name__ == "__main__":
    unittest.main()

# Metadata: [test_nexus_occ_scope]
