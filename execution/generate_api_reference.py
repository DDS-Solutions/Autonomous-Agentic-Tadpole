#!/usr/bin/env python3
"""
@docs ARCHITECTURE:Infrastructure

### AI Assist Note
**🛡️ Tadpole OS: Generate Api Reference**
Generate docs/openapi.yaml and docs/API_REFERENCE.md from server-rs/src/router.rs.

This intentionally keeps schemas shallow until route handlers expose typed
request/response metadata. The route list, auth boundary, and feature-disabled
memory behavior are generated from the Axum router source.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

ROOT = Path(__file__).resolve().parent.parent
ROUTER = ROOT / "server-rs" / "src" / "router.rs"
OPENAPI = ROOT / "docs" / "openapi.yaml"
API_REFERENCE = ROOT / "docs" / "API_REFERENCE.md"
VERSION_JSON = ROOT / "version.json"


@dataclass(frozen=True)
class Route:
    method: str
    path: str
    handler: str
    public: bool = False
    feature_note: str | None = None


NESTS = {
    "build_agent_routes": "/agents",
    "build_oversight_routes": "/oversight",
    "build_infra_routes": "/infra",
    "build_model_manager_routes": "/model-manager",
    "build_skills_routes": "/skills",
    "build_benchmark_routes": "/benchmarks",
    "build_continuity_routes": "/continuity",
    "build_docs_routes": "/docs",
    "build_system_routes": "/system",
    "build_governance_routes": "/governance",
    "build_sovereign_routes": "/sovereign",
    "build_engine_public_routes": "",
    "build_engine_protected_routes": "",
}

METHOD_ROUTER_MAP = {
    "build_agent_memory_route": [
        ("GET", "routes::memory::get_agent_memory", "Requires Cargo feature vector-memory; otherwise returns 501."),
        ("POST", "routes::memory::save_agent_memory", "Requires Cargo feature vector-memory; otherwise returns 501."),
    ],
    "build_agent_memory_delete_route": [
        ("DELETE", "routes::memory::delete_agent_memory", "Requires Cargo feature vector-memory; otherwise returns 501."),
    ],
    "build_search_memory_route": [
        ("GET", "routes::memory::global_search", "Requires Cargo feature vector-memory; otherwise returns 501."),
    ],
}

TAG_BY_PREFIX = [
    ("/v1/engine", "engine"),
    ("/v1/agents", "agents"),
    ("/v1/oversight", "oversight"),
    ("/v1/infra", "infra"),
    ("/v1/model-manager", "model-manager"),
    ("/v1/skills", "skills"),
    ("/v1/benchmarks", "benchmarks"),
    ("/v1/continuity", "continuity"),
    ("/v1/docs", "docs"),
    ("/v1/system", "system"),
    ("/v1/governance", "governance"),
    ("/v1/sovereign", "sovereign"),
    ("/v1/search/memory", "memory"),
    ("/v1/mcp", "mcp"),
    ("/v1/env-schema", "system"),
    ("/v1/api/pull", "model-manager"),
]

PUBLIC_PATHS = {
    "/v1/engine/health",
    "/v1/engine/ws",
    "/v1/engine/live-voice",
}


def current_version() -> str:
    if VERSION_JSON.exists():
        match = re.search(r'"version"\s*:\s*"([^"]+)"', VERSION_JSON.read_text(encoding="utf-8"))
        if match:
            return match.group(1)
    return "0.0.0"


def rust_path_to_openapi(path: str) -> str:
    return re.sub(r"\{([a-zA-Z0-9_]+)\}", r"{\1}", path)


def join_path(prefix: str, path: str) -> str:
    if path == "/":
        return f"/v1{prefix}" or "/v1"
    return f"/v1{prefix}{path}".replace("//", "/").rstrip("/") or "/"


def extract_function_body(source: str, fn_name: str) -> str:
    match = re.search(rf"fn\s+{re.escape(fn_name)}\s*\(", source)
    if not match:
        return ""
    brace_start = source.find("{", match.end())
    if brace_start == -1:
        return ""
    depth = 0
    in_string = False
    escaped = False
    for idx in range(brace_start, len(source)):
        char = source[idx]
        if in_string:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
            continue
        if char == '"':
            in_string = True
            continue
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return source[brace_start + 1 : idx]
    return ""


def extract_route_calls(body: str) -> Iterable[tuple[str, str, str]]:
    cursor = 0
    while True:
        start = body.find(".route", cursor)
        if start == -1:
            return
        paren = body.find("(", start)
        if paren == -1:
            return

        depth = 0
        in_string = False
        escaped = False
        end = None
        for idx in range(paren, len(body)):
            char = body[idx]
            if in_string:
                if escaped:
                    escaped = False
                elif char == "\\":
                    escaped = True
                elif char == '"':
                    in_string = False
                continue
            if char == '"':
                in_string = True
            elif char == "(":
                depth += 1
            elif char == ")":
                depth -= 1
                if depth == 0:
                    end = idx
                    break
        if end is None:
            return

        args = body[paren + 1 : end]
        cursor = end + 1

        comma = None
        depth = 0
        in_string = False
        escaped = False
        for idx, char in enumerate(args):
            if in_string:
                if escaped:
                    escaped = False
                elif char == "\\":
                    escaped = True
                elif char == '"':
                    in_string = False
                continue
            if char == '"':
                in_string = True
            elif char in "([{":
                depth += 1
            elif char in ")]}":
                depth -= 1
            elif char == "," and depth == 0:
                comma = idx
                break

        if comma is None:
            continue

        path_expr = args[:comma].strip()
        target = " ".join(args[comma + 1 :].strip().rstrip(",").split())
        path_match = re.match(r'"([^"]+)"$', path_expr)
        if path_match:
            yield path_match.group(1), target, target


def method_from_target(target: str) -> list[tuple[str, str, str | None]]:
    for builder, routes in METHOD_ROUTER_MAP.items():
        if target.startswith(builder):
            return routes

    methods = []
    method_map = {
        "get": "GET",
        "post": "POST",
        "put": "PUT",
        "delete": "DELETE",
        "patch": "PATCH",
    }

    for rust_method, http_method in method_map.items():
        direct = re.match(rf"{rust_method}\((.+)\)$", target)
        if direct:
            methods.append((http_method, direct.group(1), None))

        qualified = re.match(rf"axum::routing::{rust_method}\((.+)\)$", target)
        if qualified:
            methods.append((http_method, qualified.group(1), None))

    return methods


def discover_routes() -> list[Route]:
    source = ROUTER.read_text(encoding="utf-8")
    routes: list[Route] = []

    for fn_name, prefix in NESTS.items():
        body = extract_function_body(source, fn_name)
        for route_path, target, _raw in extract_route_calls(body):
            for method, handler, note in method_from_target(target):
                full_path = join_path(prefix, route_path)
                routes.append(
                    Route(
                        method=method,
                        path=full_path,
                        handler=handler,
                        public=full_path in PUBLIC_PATHS,
                        feature_note=note,
                    )
                )

    protected_body = extract_function_body(source, "build_protected_v1_routes")
    for route_path, target, _raw in extract_route_calls(protected_body):
        for method, handler, note in method_from_target(target):
            full_path = join_path("", route_path)
            routes.append(Route(method=method, path=full_path, handler=handler, feature_note=note))

    # Stable ordering and dedupe.
    deduped = {(route.method, route.path): route for route in routes}
    return sorted(deduped.values(), key=lambda route: (route.path, route.method))


def tag_for(path: str) -> str:
    for prefix, tag in TAG_BY_PREFIX:
        if path.startswith(prefix):
            return tag
    return "system"


def summary_for(route: Route) -> str:
    clean = route.handler.split("::")[-1]
    words = clean.replace("_handler", "").replace("_", " ")
    return words[:1].upper() + words[1:]


def path_params(path: str) -> list[str]:
    return re.findall(r"\{([a-zA-Z0-9_]+)\}", path)


def write_openapi(routes: list[Route]) -> None:
    version = current_version()
    tags = sorted({tag_for(route.path) for route in routes})
    lines = [
        "openapi: 3.0.3",
        "info:",
        "  title: Tadpole OS API",
        f"  version: {version}",
        "  description: >",
        "    API surface generated from server-rs/src/router.rs.",
        "servers:",
        "  - url: http://127.0.0.1:8000",
        "    description: Default local Rust engine",
        "security:",
        "  - bearerAuth: []",
        "tags:",
    ]
    lines.extend(f"  - name: {tag}" for tag in tags)
    lines.append("paths:")

    grouped: dict[str, list[Route]] = {}
    for route in routes:
        grouped.setdefault(rust_path_to_openapi(route.path), []).append(route)

    for path, path_routes in grouped.items():
        lines.append(f"  {path}:")
        for route in path_routes:
            lines.extend(
                [
                    f"    {route.method.lower()}:",
                    f"      tags: [{tag_for(route.path)}]",
                    f"      summary: {summary_for(route)}",
                ]
            )
            if route.public:
                lines.append("      security: []")
            params = path_params(path)
            if params:
                lines.append("      parameters:")
                for param in params:
                    lines.extend(
                        [
                            f"        - name: {param}",
                            "          in: path",
                            "          required: true",
                            "          schema: { type: string }",
                        ]
                    )
            if route.feature_note:
                lines.extend(["      description: >", f"        {route.feature_note}"])
            lines.extend(
                [
                    "      responses:",
                    '        "200":',
                    "          description: Success",
                    "          content:",
                    "            application/json:",
                    "              schema:",
                    "                $ref: '#/components/schemas/GenericResponse'",
                ]
            )
            if route.feature_note and "501" in route.feature_note:
                lines.extend(
                    [
                        '        "501":',
                        "          description: Required feature disabled",
                        "          content:",
                        "            text/plain:",
                        "              schema:",
                        "                type: string",
                    ]
                )
            if not route.public:
                lines.extend(
                    [
                        '        "401":',
                        "          description: Unauthorized",
                        "          content:",
                        "            application/json:",
                        "              schema:",
                        "                $ref: '#/components/schemas/ProblemDetails'",
                    ]
                )

    lines.extend(
        [
            "components:",
            "  securitySchemes:",
            "    bearerAuth:",
            "      type: http",
            "      scheme: bearer",
            "  schemas:",
            "    GenericResponse:",
            "      type: object",
            "      additionalProperties: true",
            "    ProblemDetails:",
            "      type: object",
            "      additionalProperties: true",
            "      properties:",
            "        type: { type: string }",
            "        title: { type: string }",
            "        status: { type: integer }",
            "        detail: { type: string }",
            "        error_code: { type: string, nullable: true }",
            "        help_link: { type: string, nullable: true }",
        ]
    )
    OPENAPI.write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_api_reference(routes: list[Route]) -> None:
    version = current_version()
    lines = [
        "# Tadpole OS API Reference",
        "",
        "> [!IMPORTANT]",
        "> **AI Assist Note (Knowledge Heritage)**:",
        '> This document is part of the "Sovereign Reality" documentation.',
        "> - **@docs ARCHITECTURE:Documentation**",
        "> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.",
        "> - **Telemetry Link**: Cross-reference with `execution/parity_guard.py` results.",
        ">",
        "> ### AI Assist Note",
        "> API reference generated from `server-rs/src/router.rs`.",
        ">",
        "> ### Debugging & Observability",
        "> Traceability via `execution/parity_guard.py`.",
        "",
        f"**Version**: {version}",
        "**Source of truth**: `server-rs/src/router.rs`",
        "",
        "The Rust engine binds to `127.0.0.1:8000` by default and nests application routes under `/v1`.",
        "",
        "## Authentication",
        "",
        "Public routes:",
        "",
    ]
    for route in routes:
        if route.public:
            lines.append(f"- `{route.method} {route.path}`")
    lines.extend(
        [
            "",
            "Protected routes require:",
            "",
            "```http",
            "Authorization: Bearer <NEURAL_TOKEN>",
            "```",
            "",
        ]
    )

    grouped: dict[str, list[Route]] = {}
    for route in routes:
        grouped.setdefault(tag_for(route.path), []).append(route)

    for tag in sorted(grouped):
        title = tag.replace("-", " ").title()
        lines.extend([f"## {title}", "", "| Method | Path | Handler | Notes |", "| --- | --- | --- | --- |"])
        for route in grouped[tag]:
            note = "Public" if route.public else "Protected"
            if route.feature_note:
                note = f"{note}; {route.feature_note}"
            lines.append(f"| `{route.method}` | `{route.path}` | `{route.handler}` | {note} |")
        lines.append("")

    lines.append("[//]: # (Metadata: [API_REFERENCE])")
    API_REFERENCE.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> None:
    routes = discover_routes()
    if not routes:
        raise SystemExit("No routes discovered from server-rs/src/router.rs")
    write_openapi(routes)
    write_api_reference(routes)
    print(f"Generated {len(routes)} routes into {OPENAPI} and {API_REFERENCE}")


if __name__ == "__main__":
    main()

# Metadata: [generate_api_reference]

# Metadata: [generate_api_reference]
