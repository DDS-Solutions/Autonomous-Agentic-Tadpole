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

    def test_validate_arguments_empty_or_none_schema(self):
        # Empty and non-dict schemas should be handled gracefully without error
        tadpole_mcp_server.validate_arguments({"any": "key"}, None)
        tadpole_mcp_server.validate_arguments({"any": "key"}, {})
        tadpole_mcp_server.validate_arguments({}, {})

    def test_mcp_import_resilience(self):
        # Ensure HAS_MCP flag exists and module is safely loaded
        self.assertTrue(hasattr(tadpole_mcp_server, "HAS_MCP"))
        self.assertIsInstance(tadpole_mcp_server.HAS_MCP, bool)
        self.assertTrue(callable(tadpole_mcp_server.validate_arguments))

    def test_decorators_resilient_without_list_tools(self):
        list_dec = tadpole_mcp_server._list_tools_decorator()
        call_dec = tadpole_mcp_server._call_tool_decorator()
        self.assertTrue(callable(list_dec))
        self.assertTrue(callable(call_dec))
        
        @list_dec
        def dummy_list():
            return []
            
        @call_dec
        def dummy_call():
            return []
            
        self.assertEqual(dummy_list(), [])
        self.assertEqual(dummy_call(), [])

    def test_sandbox_blocks_shell_operators(self):
        import asyncio
        tadpole_mcp_server._TOOL_MANIFESTS["bad_shell"] = {
            "name": "bad_shell",
            "execution_command": "python script.py | rm -rf /"
        }
        res = asyncio.run(tadpole_mcp_server.handle_call_tool("bad_shell", {}))
        self.assertIn("Shell Scanner compliance", str(res[0]))

    def test_sandbox_blocks_disallowed_flags(self):
        import asyncio
        tadpole_mcp_server._TOOL_MANIFESTS["bad_flag"] = {
            "name": "bad_flag",
            "execution_command": "python -m http.server"
        }
        res = asyncio.run(tadpole_mcp_server.handle_call_tool("bad_flag", {}))
        self.assertIn("Inline command evaluation and module execution flags", str(res[0]))

    def test_sandbox_blocks_unauthorized_executable(self):
        import asyncio
        tadpole_mcp_server._TOOL_MANIFESTS["bad_exe"] = {
            "name": "bad_exe",
            "execution_command": "bash /bin/evil.sh"
        }
        res = asyncio.run(tadpole_mcp_server.handle_call_tool("bad_exe", {}))
        self.assertIn("not in the system allowlist", str(res[0]))

    def test_sandbox_blocks_outside_workspace(self):
        import asyncio
        tadpole_mcp_server._TOOL_MANIFESTS["bad_path"] = {
            "name": "bad_path",
            "execution_command": "python ../../outside.py"
        }
        res = asyncio.run(tadpole_mcp_server.handle_call_tool("bad_path", {}))
        self.assertIn("resides outside workspace boundary", str(res[0]))

    def test_sandbox_default_deny_host_execution(self):
        import asyncio
        import os
        tadpole_mcp_server._TOOL_MANIFESTS["valid_skill"] = {
            "name": "valid_skill",
            "execution_command": "python execution/parity_guard.py"
        }
        # By default (ALLOW_HOST_SKILL_EXECUTION unset or false), host execution is blocked
        old_val = os.environ.get("ALLOW_HOST_SKILL_EXECUTION")
        try:
            if "ALLOW_HOST_SKILL_EXECUTION" in os.environ:
                del os.environ["ALLOW_HOST_SKILL_EXECUTION"]
            res = asyncio.run(tadpole_mcp_server.handle_call_tool("valid_skill", {}))
            self.assertIn("Unsandboxed host execution is disabled by default", str(res[0]))
        finally:
            if old_val is not None:
                os.environ["ALLOW_HOST_SKILL_EXECUTION"] = old_val

if __name__ == "__main__":
    unittest.main()

# Metadata: [test_mcp_sandbox]
