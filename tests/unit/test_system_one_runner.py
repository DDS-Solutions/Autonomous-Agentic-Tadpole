"""
@docs ARCHITECTURE:Testing
@docs ARCHITECTURE:Agent:SystemOne

### AI Assist Note
**test_system_one_runner**: Unittest runner wrapper for Tadpole System 1 Decision Engine.
Runs the three-path adversarial integration suite under python -m unittest discovery.

### 🔍 Debugging & Observability
- **Failure Path**: System 1 calibration drift or injection gate failure.
- **Telemetry Link**: Search `[test_system_one_runner]` in test output.
"""

import unittest
import sys
import importlib.util
from pathlib import Path

EXEC_DIR = Path(__file__).resolve().parent.parent.parent / "execution"
TEST_SCRIPT = EXEC_DIR / "tests" / "test_system_one_integration.py"

# Dynamically load the integration module to avoid namespace collision
spec = importlib.util.spec_from_file_location("tadpole_sys1_integration", str(TEST_SCRIPT))
sys1_mod = importlib.util.module_from_spec(spec)
sys.modules["tadpole_sys1_integration"] = sys1_mod
spec.loader.exec_module(sys1_mod)

class TestSystemOneIntegration(unittest.TestCase):
    def test_happy_path(self):
        sys1_mod.test_happy_path_routing_and_triage()

    def test_failure_path_injection(self):
        sys1_mod.test_failure_path_injection_and_oversight_routing()

    def test_edge_case_scripts_and_shortlisting(self):
        sys1_mod.test_edge_case_script_detection_and_shortlisting()

if __name__ == "__main__":
    unittest.main()

# Metadata: [test_system_one_runner]
