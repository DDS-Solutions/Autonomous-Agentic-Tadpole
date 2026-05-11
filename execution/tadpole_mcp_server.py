import asyncio
import json
import os
import subprocess
from pathlib import Path
from typing import Any, Sequence

from mcp.server.models import InitializationOptions
import mcp.types as types
from mcp.server import NotificationOptions, Server
from mcp.server.stdio import stdio_server
from core.registry import SkillRegistry


# Initialize the MCP server
server = Server("tadpole-execution-layer")

# Store tools globally
_TOOLS_CACHE = []
_TOOL_MANIFESTS = {}
_SKILL_REGISTRY = SkillRegistry()


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

            _TOOLS_CACHE.append(
                types.Tool(
                    name=name,
                    description=description,
                    inputSchema=schema
                )
            )
            _TOOL_MANIFESTS[name] = manifest

        except Exception as e:
            pass

    # 2. Load Modular Class-based Skills
    _SKILL_REGISTRY.discover_skills()
    for tool_def in _SKILL_REGISTRY.get_all_tools():
        _TOOLS_CACHE.append(
            types.Tool(
                name=tool_def["name"],
                description=tool_def["description"],
                inputSchema=tool_def["schema"]
            )
        )


@server.list_tools()
async def handle_list_tools() -> list[types.Tool]:
    """Returns the list of parsed tools."""
    return _TOOLS_CACHE

@server.call_tool()
async def handle_call_tool(
    name: str, arguments: dict | None
) -> list[types.TextContent | types.ImageContent | types.EmbeddedResource]:
    """Executes a specific tool."""
    import time
    import sys
    start_time = time.perf_counter()

    # 1. Try Modular Class-based Registry first (Faster/Modern)
    if name in _SKILL_REGISTRY.skills:
        result = await _SKILL_REGISTRY.call_skill(name, arguments or {})
        return [types.TextContent(type="text", text=result)]

    # 2. Fallback to Legacy JSON Manifests
    if name not in _TOOL_MANIFESTS:
        raise ValueError(f"Tool not found: {name}")

    # Log Deprecation Warning
    print(f"⚠️ [MCP] DEPRECATION WARNING: Tool '{name}' is running in legacy subprocess mode. Consider migrating to BaseSkill.", file=sys.stderr)

    manifest = _TOOL_MANIFESTS[name]
    command = manifest.get("execution_command")

    if not command:
        return [types.TextContent(type="text", text=f"Tool {name} has no execution_command defined.")]

    if command == "(Native Execution Mode)":
        return [types.TextContent(type="text", text=f"Tool {name} is a native Rust tool. Please execute via TadpoleOS internal host.")]

    args_json = json.dumps(arguments or {})
    env = os.environ.copy()
    env["TADPOLE_SKILL_ARGS"] = args_json

    workspace_root = os.environ.get("WORKSPACE_ROOT", os.getcwd())

    try:
        process = await asyncio.create_subprocess_shell(
            command,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            env=env,
            cwd=workspace_root
        )

        stdout, stderr = await asyncio.wait_for(process.communicate(), timeout=60.0)

        stdout_str = stdout.decode('utf-8', errors='replace')
        stderr_str = stderr.decode('utf-8', errors='replace')

        duration = (time.perf_counter() - start_time) * 1000
        print(f"🕒 [MCP] Legacy Tool '{name}' executed in {duration:.2f}ms (Subprocess)", file=sys.stderr)

        if process.returncode == 0:
            return [types.TextContent(type="text", text=stdout_str)]
        else:
            return [types.TextContent(type="text", text=f"Execution Failed (Code {process.returncode}):\n{stdout_str}\n{stderr_str}")]

    except asyncio.TimeoutError:
        return [types.TextContent(type="text", text="Execution timed out after 60 seconds.")]
    except Exception as e:
        return [types.TextContent(type="text", text=f"Execution Error: {str(e)}")]


async def main():
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
