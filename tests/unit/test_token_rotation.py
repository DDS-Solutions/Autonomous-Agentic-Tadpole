"""
@docs ARCHITECTURE:Core

### AI Assist Note
**test_token_rotation**: Core technical resource for the Tadpole OS infrastructure.

### 🔍 Debugging & Observability
- **Failure Path**: Script crash or unexpected exception.
- **Telemetry Link**: Search `[test_token_rotation]` in system logs.
"""

import unittest
import os
import shutil
import tempfile
from pathlib import Path
import sys

# Add execution directory to sys.path so we can import rotate_token
sys.path.append(str(Path(__file__).parent.parent.parent / "execution"))

import rotate_token

class TestTokenRotation(unittest.TestCase):
    def setUp(self):
        # Create temp dir and temp .env file
        self.test_dir = tempfile.mkdtemp()
        self.env_file = Path(self.test_dir) / ".env"
        self.env_file.write_text("NEURAL_TOKEN=original-secret-123\nOTHER_VAR=abc\n")
        
        # Override the module's ENV_FILE constant
        self.original_env_file = rotate_token.ENV_FILE
        rotate_token.ENV_FILE = self.env_file

    def tearDown(self):
        # Clean up temp files
        shutil.rmtree(self.test_dir)
        # Restore module constant
        rotate_token.ENV_FILE = self.original_env_file

    def test_rotate_token_grace_period(self):
        # Trigger rotation
        rotate_token.rotate_token(grace_period_secs=10)
        
        # Read back env
        lines = self.env_file.read_text().splitlines()
        
        # Check values
        env_dict = {}
        for line in lines:
            if "=" in line:
                k, v = line.split("=", 1)
                env_dict[k.strip()] = v.strip()

        self.assertEqual(env_dict["NEURAL_TOKEN_OLD"], "original-secret-123")
        self.assertEqual(env_dict["OTHER_VAR"], "abc")
        self.assertTrue("NEURAL_TOKEN" in env_dict)
        self.assertTrue("NEURAL_TOKEN_NEW" in env_dict)
        self.assertEqual(env_dict["NEURAL_TOKEN"], env_dict["NEURAL_TOKEN_NEW"])
        self.assertNotEqual(env_dict["NEURAL_TOKEN"], "original-secret-123")

    def test_confirm_rotation_revokes_old(self):
        # Initiate rotation
        rotate_token.rotate_token(grace_period_secs=10)
        
        # Confirm rotation
        rotate_token.confirm_rotation()
        
        # Read back env
        lines = self.env_file.read_text().splitlines()
        env_dict = {}
        for line in lines:
            if "=" in line:
                k, v = line.split("=", 1)
                env_dict[k.strip()] = v.strip()

        self.assertFalse("NEURAL_TOKEN_OLD" in env_dict)
        self.assertFalse("NEURAL_TOKEN_NEW" in env_dict)
        self.assertEqual(env_dict["OTHER_VAR"], "abc")
        self.assertTrue("NEURAL_TOKEN" in env_dict)

if __name__ == "__main__":
    unittest.main()

# Metadata: [test_token_rotation]
