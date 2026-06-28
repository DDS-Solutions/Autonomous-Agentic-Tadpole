"""
@docs ARCHITECTURE:Core

### AI Assist Note
**test_mcp_sandbox**: Core technical resource for the Tadpole OS infrastructure.

### 🔍 Debugging & Observability
- **Failure Path**: Script crash or unexpected exception.
- **Telemetry Link**: Search `[test_mcp_sandbox]` in system logs.
"""

import unittest
import sys
from pathlib import Path

# Add execution directory to sys.path
sys.path.append(str(Path(__file__).parent.parent.parent / "execution"))

import tadpole_mcp_server

class TestMcpSandbox(unittest.TestCase):
    def test_validate_arguments_success(self):
        schema = {
            "type": "object",
            "required": ["symbol_name", "retries"],
            "properties": {
                "symbol_name": {"type": "string"},
                "retries": {"type": "integer"},
                "timeout": {"type": "number"},
                "verbose": {"type": "boolean"},
                "items": {"type": "array"},
                "config": {"type": "object"}
            }
        }
        
        args = {
            "symbol_name": "TestClass",
            "retries": 3,
            "timeout": 15.5,
            "verbose": True,
            "items": [1, 2, 3],
            "config": {"key": "val"}
        }
        
        # Should not raise any exceptions
        try:
            tadpole_mcp_server.validate_arguments(args, schema)
        except Exception as e:
            self.fail(f"validate_arguments raised unexpected exception: {e}")

    def test_validate_arguments_missing_required(self):
        schema = {
            "type": "object",
            "required": ["symbol_name"],
            "properties": {
                "symbol_name": {"type": "string"}
            }
        }
        args = {}
        with self.assertRaises(ValueError) as ctx:
            tadpole_mcp_server.validate_arguments(args, schema)
        self.assertIn("Missing required parameter", str(ctx.exception))

    def test_validate_arguments_type_mismatch(self):
        schema = {
            "type": "object",
            "properties": {
                "retries": {"type": "integer"},
                "verbose": {"type": "boolean"}
            }
        }
        
        # retries is string instead of integer
        with self.assertRaises(TypeError) as ctx:
            tadpole_mcp_server.validate_arguments({"retries": "3"}, schema)
        self.assertIn("must be an integer", str(ctx.exception))
        
        # verbose is int instead of boolean
        with self.assertRaises(TypeError) as ctx:
            tadpole_mcp_server.validate_arguments({"verbose": 1}, schema)
        self.assertIn("must be a boolean", str(ctx.exception))

if __name__ == "__main__":
    unittest.main()

# Metadata: [test_mcp_sandbox]
