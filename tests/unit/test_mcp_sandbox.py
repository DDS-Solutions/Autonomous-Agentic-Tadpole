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


    def test_build_skill_subprocess_env_excludes_secrets(self):
        source = {
            "PATH": "/usr/bin",
            "HOME": "/home/tadpole",
            "WORKSPACE_ROOT": "/workspace",
            "OPENAI_API_KEY": "sk-secret",
            "ANTHROPIC_API_KEY": "sk-ant-secret",
            "GOOGLE_API_KEY": "goog-secret",
            "GROQ_API_KEY": "groq-secret",
            "DEEPSEEK_API_KEY": "ds-secret",
            "REPLICATE_API_KEY": "r8-secret",
            "NEURAL_TOKEN": "neural-secret",
            "NEURAL_TOKEN_OLD": "old-secret",
            "NEURAL_TOKEN_NEW": "new-secret",
            "NEURAL_ENGINE_ACCESS_TOKEN": "engine-secret",
            "UNRELATED_CUSTOM": "should-not-pass",
        }
        env = tadpole_mcp_server.build_skill_subprocess_env(
            arguments_json='{"x":1}',
            source_env=source,
        )
        self.assertEqual(env["PATH"], "/usr/bin")
        self.assertEqual(env["HOME"], "/home/tadpole")
        self.assertEqual(env["WORKSPACE_ROOT"], "/workspace")
        self.assertEqual(env["TADPOLE_SKILL_ARGS"], '{"x":1}')
        for secret_key in (
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
            "GOOGLE_API_KEY",
            "GROQ_API_KEY",
            "DEEPSEEK_API_KEY",
            "REPLICATE_API_KEY",
            "NEURAL_TOKEN",
            "NEURAL_TOKEN_OLD",
            "NEURAL_TOKEN_NEW",
            "NEURAL_ENGINE_ACCESS_TOKEN",
            "UNRELATED_CUSTOM",
        ):
            self.assertNotIn(secret_key, env)

if __name__ == "__main__":
    unittest.main()

# Metadata: [test_mcp_sandbox]
