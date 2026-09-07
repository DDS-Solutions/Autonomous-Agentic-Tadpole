"""
@docs ARCHITECTURE:Infrastructure:Execution

### AI Assist Note
**🛡️ Tadpole OS: MCP Execution Host**
This host orchestrates modular and legacy skill execution for the Tadpole OS agent swarm. 
Uses stdio to communicate with clients, discovering class-based skills via the SkillRegistry.

### 🔍 Debugging & Observability
- **Failure Path**: Failed stdio streams, missing skill manifest JSON, or subprocess timeouts.
- **Telemetry Link**: Search `[tadpole_mcp_server]` in system logs.
"""

from __future__ import annotations

import asyncio
import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any, Sequence

try:
    from mcp.server.models import InitializationOptions
    import mcp.types as types
    from mcp.server import NotificationOptions, Server
    from mcp.server.stdio import stdio_server
    HAS_MCP = True
except ImportError:
    HAS_MCP = False
    types = None
    Server = None
    InitializationOptions = None
    NotificationOptions = None
    stdio_server = None

from core.registry import SkillRegistry


# Initialize the MCP server
if HAS_MCP and Server is not None:
    server = Server("tadpole-execution-layer")
else:
    server = None


def _list_tools_decorator():
    if server is not None:
        return server.list_tools()
    def decorator(func):
        return func
    return decorator


def _call_tool_decorator():
    if server is not None:
        return server.call_tool()
    def decorator(func):
        return func
    return decorator

# Store tools globally
_TOOLS_CACHE = []
_TOOL_MANIFESTS = {}
_SKILL_REGISTRY = SkillRegistry()


def validate_arguments(args: dict, schema: dict):
    if not schema or not isinstance(schema, dict):
        return
    properties = schema.get("properties", {})
    required = schema.get("required", [])
    
    # Check required fields
    for req in required:
        if req not in args:
            raise ValueError(f"Missing required parameter: {req}")
            
    # Check types
    for key, val in args.items():
        if key in properties:
            prop_type = properties[key].get("type")
            if prop_type == "string" and not isinstance(val, str):
                raise TypeError(f"Parameter '{key}' must be a string")
            elif prop_type == "integer" and not isinstance(val, int):
                raise TypeError(f"Parameter '{key}' must be an integer")
            elif prop_type == "number" and not isinstance(val, (int, float)):
                raise TypeError(f"Parameter '{key}' must be a number")
            elif prop_type == "boolean" and not isinstance(val, bool):
                raise TypeError(f"Parameter '{key}' must be a boolean")
            elif prop_type == "array" and not isinstance(val, list):
                raise TypeError(f"Parameter '{key}' must be an array")
            elif prop_type == "object" and not isinstance(val, dict):
                raise TypeError(f"Parameter '{key}' must be an object")



def load_skills():
    """Scans the execution directory for JSON manifests and loads them."""
    global _TOOLS_CACHE, _TOOL_MANIFESTS
    _TOOLS_CACHE.clear()
    _TOOL_MANIFESTS.clear()

    execution_dir = Path(__file__).parent

    # 1. Load Legacy JSON Skills
    for json_file in execution_dir.glob("*.json"):
        try:
            with open(json_file, 'r', encoding='utf-8') as f:
                manifest = json.load(f)

            name = manifest.get("name")
            description = manifest.get("description", "TadpoleOS Execution Skill")
            schema = manifest.get("schema", {"type": "object", "properties": {}})

            if not name:
                continue

            if HAS_MCP and types is not None:
                _TOOLS_CACHE.append(
                    types.Tool(
                        name=name,
                        description=description,
                        inputSchema=schema
                    )
                )
            else:
                _TOOLS_CACHE.append(
                    {
                        "name": name,
                        "description": description,
                        "inputSchema": schema
                    }
                )
            _TOOL_MANIFESTS[name] = manifest

        except Exception as e:
            pass

    # 2. Load Modular Class-based Skills
    _SKILL_REGISTRY.discover_skills()
    for tool_def in _SKILL_REGISTRY.get_all_tools():
        if HAS_MCP and types is not None:
            _TOOLS_CACHE.append(
                types.Tool(
                    name=tool_def["name"],
                    description=tool_def["description"],
                    inputSchema=tool_def["schema"]
                )
            )
        else:
            _TOOLS_CACHE.append(tool_def)


def _format_text_response(text: str) -> Any:
    if HAS_MCP and types is not None:
        return types.TextContent(type="text", text=text)
    return {"type": "text", "text": text}


@_list_tools_decorator()
async def handle_list_tools() -> list[Any]:
    """Returns the list of parsed tools."""
    return _TOOLS_CACHE

@_call_tool_decorator()
async def handle_call_tool(
    name: str, arguments: dict | None
) -> list[Any]:
    """Executes a specific tool."""
    import time
    import sys
    start_time = time.perf_counter()

    # 1. Try Modular Class-based Registry first (Faster/Modern)
    if name in _SKILL_REGISTRY.skills:
        result = await _SKILL_REGISTRY.call_skill(name, arguments or {})
        return [_format_text_response(result)]

    # 2. Fallback to Legacy JSON Manifests
    if name not in _TOOL_MANIFESTS:
        raise ValueError(f"Tool not found: {name}")

    # Log Deprecation Warning
    print(f"⚠️ [MCPHost] DEPRECATION WARNING: Tool '{name}' is running in legacy subprocess mode. Consider migrating to BaseSkill.", file=sys.stderr)

    manifest = _TOOL_MANIFESTS[name]
    command = manifest.get("execution_command")

    if not command:
        return [_format_text_response(f"Tool {name} has no execution_command defined.")]

    if command == "(Native Execution Mode)":
        return [_format_text_response(f"Tool {name} is a native Rust tool. Please execute via TadpoleOS internal host.")]

    # Shell Scanner Compliance (SEC-05) - Extra safety guard
    dangerous_chars = ['|', '>', '<', '&', ';', '`', '$(']
    if any(char in command for char in dangerous_chars):
        return [_format_text_response("Execution Failed: Command failed Shell Scanner compliance (contains forbidden shell operators).")]

    # Validate arguments against manifest schema
    schema = manifest.get("schema", {})
    try:
        validate_arguments(arguments or {}, schema)
    except Exception as err:
        return [_format_text_response(f"Argument Validation Failed: {str(err)}")]

    args_json = json.dumps(arguments or {})
    env = os.environ.copy()
    env["TADPOLE_SKILL_ARGS"] = args_json

    workspace_root = os.environ.get("WORKSPACE_ROOT", os.getcwd())

    # Split command safely (shlex) and run directly without shell
    import shlex
    cmd_parts = shlex.split(command)
    if cmd_parts and cmd_parts[0] == "python":
        cmd_parts[0] = sys.executable

    def set_limits():
        # Set Linux/Unix limits
        if os.name != 'nt':
            try:
                import resource
                # 30 CPU seconds limit
                resource.setrlimit(resource.RLIMIT_CPU, (30, 30))
                # 256MB Address Space memory limit
                resource.setrlimit(resource.RLIMIT_AS, (256 * 1024 * 1024, 256 * 1024 * 1024))
            except Exception:
                pass

    try:
        process = await asyncio.create_subprocess_exec(
            *cmd_parts,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            env=env,
            cwd=workspace_root,
            preexec_fn=set_limits if os.name != 'nt' else None
        )

        stdout, stderr = await asyncio.wait_for(process.communicate(), timeout=30.0)

        stdout_str = stdout.decode('utf-8', errors='replace')
        stderr_str = stderr.decode('utf-8', errors='replace')

        duration = (time.perf_counter() - start_time) * 1000
        print(f"🕒 [MCPHost] Legacy Tool '{name}' executed in {duration:.2f}ms (Subprocess)", file=sys.stderr)

        if process.returncode == 0:
            return [_format_text_response(stdout_str)]
        else:
            return [_format_text_response(f"Execution Failed (Code {process.returncode}):\n{stdout_str}\n{stderr_str}")]

    except asyncio.TimeoutError:
        try:
            process.kill()
            await process.wait()
        except Exception:
            pass
        return [_format_text_response("Execution timed out after 30 seconds (subprocess terminated).")]
    except Exception as e:
        return [_format_text_response(f"Execution Error: {str(e)}")]


async def main():
    if not HAS_MCP or server is None or stdio_server is None:
        print("❌ Error: The 'mcp' Python SDK is required to run the Tadpole MCP Server.", file=sys.stderr)
        print("Please install requirements: pip install -r execution/requirements.txt", file=sys.stderr)
        sys.exit(1)

    # Load all skills into memory
    load_skills()

    # Run the server via STDIO
    async with stdio_server() as (read_stream, write_stream):
        await server.run(
            read_stream,
            write_stream,
            InitializationOptions(
                server_name="tadpole-execution-layer",
                server_version="1.0.0",
                capabilities=server.get_capabilities(
                    NotificationOptions(),
                    experimental_capabilities={},
                ),
            ),
        )

if __name__ == "__main__":
    asyncio.run(main())





# Metadata: [tadpole_mcp_server]
