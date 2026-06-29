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
- **Telemetry Link**: Search `[generate_api_reference]` in system logs.
"""

from __future__ import annotations

import json
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
    res = f"/v1{prefix}{path}".replace("//", "/").rstrip("/")
    return res if res else "/"


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


def load_api_metadata() -> tuple[dict[tuple[str, str], str], dict[tuple[str, str], str]]:
    metadata_file = ROOT / "execution" / "api_metadata.json"
    req_map = {}
    resp_map = {}
    if metadata_file.exists():
        try:
            data = json.loads(metadata_file.read_text(encoding="utf-8"))
            for key, val in data.get("request_body_map", {}).items():
                parts = key.split(maxsplit=1)
                if len(parts) == 2:
                    req_map[(parts[0], parts[1])] = val
            for key, val in data.get("success_response_map", {}).items():
                parts = key.split(maxsplit=1)
                if len(parts) == 2:
                    resp_map[(parts[0], parts[1])] = val
        except Exception as e:
            print(f"Warning: Failed to load api_metadata.json: {e}")
    return req_map, resp_map


REQUEST_BODY_MAP, SUCCESS_RESPONSE_MAP = load_api_metadata()


def summary_for(route: Route) -> str:
    if route.method == "PUT" and "/v1/skills/" in route.path:
        if "post_hook" in route.handler:
            return "Save hook"
        if "post_script" in route.handler:
            return "Save script"
        if "post_workflow" in route.handler:
            return "Save workflow"

    clean = route.handler.split("::")[-1]
    words = clean.replace("_handler", "").replace("_", " ")
    return words[:1].upper() + words[1:]


def escape_yaml_string(s: str) -> str:
    escaped = s.replace('\\', '\\\\').replace('"', '\\"')
    return f'"{escaped}"'


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
                    f"      summary: {escape_yaml_string(summary_for(route))}",
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

            # Request Body
            req_schema = REQUEST_BODY_MAP.get((route.method, path))
            if req_schema:
                lines.extend(
                    [
                        "      requestBody:",
                        "        required: true",
                        "        content:",
                        "          application/json:",
                        "            schema:",
                        f"              $ref: '#/components/schemas/{req_schema}'",
                    ]
                )
            elif path == "/v1/engine/transcribe":
                lines.extend(
                    [
                        "      requestBody:",
                        "        required: true",
                        "        content:",
                        "          multipart/form-data:",
                        "            schema:",
                        "              type: object",
                        "              required: [file]",
                        "              properties:",
                        "                file:",
                        "                  type: string",
                        "                  format: binary",
                    ]
                )
            elif path == "/v1/skills/import":
                lines.extend(
                    [
                        "      requestBody:",
                        "        required: true",
                        "        content:",
                        "          multipart/form-data:",
                        "            schema:",
                        "              type: object",
                        "              required: [file]",
                        "              properties:",
                        "                file:",
                        "                  type: string",
                        "                  format: binary",
                    ]
                )

            # Responses
            success_schema = SUCCESS_RESPONSE_MAP.get((route.method, path), "GenericResponse")
            lines.extend(
                [
                    "      responses:",
                    '        "200":',
                    "          description: Success",
                    "          content:",
                    "            application/json:",
                    "              schema:",
                    f"                $ref: '#/components/schemas/{success_schema}'",
                ]
            )

            # 400 Bad Request for POST/PUT
            if route.method in ("POST", "PUT"):
                lines.extend(
                    [
                        '        "400":',
                        "          description: Bad Request",
                        "          content:",
                        "            application/json:",
                        "              schema:",
                        "                $ref: '#/components/schemas/ProblemDetails'",
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
                        '        "403":',
                        "          description: Forbidden",
                        "          content:",
                        "            application/json:",
                        "              schema:",
                        "                $ref: '#/components/schemas/ProblemDetails'",
                    ]
                )

            if params:
                lines.extend(
                    [
                        '        "404":',
                        "          description: Not Found",
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
            "      required: [type, title, status, detail, severity]",
            "      properties:",
            "        type: { type: string }",
            "        title: { type: string }",
            "        status: { type: integer }",
            "        detail: { type: string }",
            "        instance: { type: string, nullable: true }",
            "        error_code: { type: string, nullable: true }",
            "        help_link: { type: string, nullable: true }",
            "        severity: { type: string }",
            "    AgentResponse:",
            "      type: object",
            "      required: [id, name, role, department, status, model, provider, budgetUsd, costUsd, isHealthy, isBankrupt, skills, version]",
            "      properties:",
            "        id: { type: string }",
            "        name: { type: string }",
            "        role: { type: string }",
            "        department: { type: string }",
            "        status: { type: string }",
            "        model: { type: string }",
            "        provider: { type: string }",
            "        budgetUsd: { type: number }",
            "        costUsd: { type: number }",
            "        isHealthy: { type: boolean }",
            "        isBankrupt: { type: boolean }",
            "        skills:",
            "          type: array",
            "          items: { type: string }",
            "        createdAt: { type: string, format: date-time, nullable: true }",
            "        version: { type: integer }",
            "    AgentResponseList:",
            "      type: object",
            "      required: [data, page, perPage, total, totalPages, _links]",
            "      properties:",
            "        data:",
            "          type: array",
            "          items: { $ref: '#/components/schemas/AgentResponse' }",
            "        page: { type: integer }",
            "        perPage: { type: integer }",
            "        total: { type: integer }",
            "        totalPages: { type: integer }",
            "        _links:",
            "          type: object",
            "          additionalProperties:",
            "            $ref: '#/components/schemas/HateoasLink'",
            "    HateoasLink:",
            "      type: object",
            "      required: [href]",
            "      properties:",
            "        href: { type: string }",
            "        method: { type: string, nullable: true }",
            "    EngineAgent:",
            "      type: object",
            "      required: [identity, models, economics, health, capabilities, state]",
            "      properties:",
            "        identity: { $ref: '#/components/schemas/AgentIdentity' }",
            "        models: { $ref: '#/components/schemas/AgentModels' }",
            "        economics: { $ref: '#/components/schemas/AgentEconomics' }",
            "        health: { $ref: '#/components/schemas/AgentHealth' }",
            "        capabilities: { $ref: '#/components/schemas/AgentCapabilities' }",
            "        state: { $ref: '#/components/schemas/AgentState' }",
            "        metadata: { type: object, additionalProperties: true }",
            "        createdAt: { type: string, format: date-time, nullable: true }",
            "        requiresOversight: { type: boolean }",
            "        voiceId: { type: string, nullable: true }",
            "        voiceEngine: { type: string, nullable: true }",
            "        sttEngine: { type: string, nullable: true }",
            "        connectorConfigs:",
            "          type: array",
            "          items: { type: object }",
            "        version: { type: integer }",
            "    AgentIdentity:",
            "      type: object",
            "      required: [id, name, role, department, description, category]",
            "      properties:",
            "        id: { type: string }",
            "        name: { type: string }",
            "        role: { type: string }",
            "        department: { type: string }",
            "        description: { type: string }",
            "        category: { type: string }",
            "        themeColor: { type: string, nullable: true }",
            "    AgentEconomics:",
            "      type: object",
            "      required: [budgetUsd, costUsd, tokensUsed, tokenUsage]",
            "      properties:",
            "        budgetUsd: { type: number }",
            "        costUsd: { type: number }",
            "        tokensUsed: { type: integer }",
            "        tokenUsage: { $ref: '#/components/schemas/TokenUsage' }",
            "    TokenUsage:",
            "      type: object",
            "      required: [input, output, total]",
            "      properties:",
            "        input: { type: integer }",
            "        output: { type: integer }",
            "        total: { type: integer }",
            "    AgentHealth:",
            "      type: object",
            "      required: [status, failureCount]",
            "      properties:",
            "        status: { type: string }",
            "        failureCount: { type: integer }",
            "        lastFailureAt: { type: string, format: date-time, nullable: true }",
            "        heartbeatAt: { type: string, format: date-time, nullable: true }",
            "    AgentModels:",
            "      type: object",
            "      required: [model]",
            "      properties:",
            "        modelId: { type: string, nullable: true }",
            "        model: { $ref: '#/components/schemas/ModelConfig' }",
            "        model2: { type: string, nullable: true }",
            "        model3: { type: string, nullable: true }",
            "        modelConfig2: { $ref: '#/components/schemas/ModelConfig', nullable: true }",
            "        modelConfig3: { $ref: '#/components/schemas/ModelConfig', nullable: true }",
            "        activeModelSlot: { type: integer, nullable: true }",
            "    ModelConfig:",
            "      type: object",
            "      required: [provider, modelId]",
            "      properties:",
            "        provider: { type: string }",
            "        modelId: { type: string }",
            "        apiKey: { type: string, nullable: true }",
            "        temperature: { type: number, nullable: true }",
            "        baseUrl: { type: string, nullable: true }",
            "        reasoningDepth: { type: integer, nullable: true }",
            "        actThreshold: { type: number, nullable: true }",
            "    AgentCapabilities:",
            "      type: object",
            "      required: [skills, workflows, mcpTools]",
            "      properties:",
            "        skills:",
            "          type: array",
            "          items: { type: string }",
            "        workflows:",
            "          type: array",
            "          items: { type: string }",
            "        mcpTools:",
            "          type: array",
            "          items: { type: string }",
            "        skillManifest: { type: object, nullable: true }",
            "    AgentState:",
            "      type: object",
            "      required: [workingMemory, currentReasoningTurn]",
            "      properties:",
            "        activeMission: { type: object, nullable: true }",
            "        currentTask: { type: string, nullable: true }",
            "        workingMemory: { type: object }",
            "        currentReasoningTurn: { type: integer }",
            "    AgentConfigUpdate:",
            "      type: object",
            "      properties:",
            "        name: { type: string }",
            "        role: { type: string }",
            "        department: { type: string }",
            "        provider: { type: string }",
            "        modelId: { type: string }",
            "        modelConfig: { $ref: '#/components/schemas/ModelConfig' }",
            "        model2: { type: string }",
            "        model3: { type: string }",
            "        apiKey: { type: string }",
            "        systemPrompt: { type: string }",
            "        temperature: { type: number }",
            "        baseUrl: { type: string }",
            "        reasoningDepth: { type: integer }",
            "        actThreshold: { type: number }",
            "        themeColor: { type: string }",
            "        budgetUsd: { type: number }",
            "        externalId: { type: string }",
            "        skills:",
            "          type: array",
            "          items: { type: string }",
            "        workflows:",
            "          type: array",
            "          items: { type: string }",
            "        mcpTools:",
            "          type: array",
            "          items: { type: string }",
            "        activeModelSlot: { type: integer }",
            "        modelConfig2: { $ref: '#/components/schemas/ModelConfig' }",
            "        modelConfig3: { $ref: '#/components/schemas/ModelConfig' }",
            "        voiceId: { type: string }",
            "        voiceEngine: { type: string }",
            "        sttEngine: { type: string }",
            "        category: { type: string }",
            "        requiresOversight: { type: boolean }",
            "        connectorConfigs:",
            "          type: array",
            "          items: { type: object }",
            "        metadata: { type: object, additionalProperties: true }",
            "    TaskPayload:",
            "      type: object",
            "      required: [task]",
            "      properties:",
            "        task: { type: string }",
            "        traceparent: { type: string, nullable: true }",
            "    TaskAcceptedResponse:",
            "      type: object",
            "      required: [status, agent_id]",
            "      properties:",
            "        status: { type: string }",
            "        agent_id: { type: string }",
            "    SpeechRequest:",
            "      type: object",
            "      required: [text]",
            "      properties:",
            "        text: { type: string }",
            "        voice: { type: string, nullable: true }",
            "        engine: { type: string, nullable: true }",
            "    SpeechResponse:",
            "      type: object",
            "      required: [status, message]",
            "      properties:",
            "        status: { type: string }",
            "        message: { type: string }",
            "    TranscribeResponse:",
            "      type: object",
            "      required: [status, text]",
            "      properties:",
            "        status: { type: string }",
            "        text: { type: string }",
            "    RegisterPayload:",
            "      type: object",
            "      required: [type, data, category]",
            "      properties:",
            "        type: { type: string }",
            "        data: { type: object }",
            "        category: { type: string }",
            "    ResolveProposalPayload:",
            "      type: object",
            "      required: [decision]",
            "      properties:",
            "        decision: { type: string }",
            "        comments: { type: string, nullable: true }",
            "    PromotePayload:",
            "      type: object",
            "      required: [name, description, capType, content, agentId]",
            "      properties:",
            "        name: { type: string }",
            "        description: { type: string }",
            "        capType: { type: string }",
            "        content: { type: string }",
            "        agentId: { type: string }",
            "        missionId: { type: string, nullable: true }",
            "    DeployResponse:",
            "      type: object",
            "      required: [status, revision]",
            "      properties:",
            "        status: { type: string }",
            "        revision: { type: string }",
            "    GenericSuccessResponse:",
            "      type: object",
            "      required: [status]",
            "      properties:",
            "        status: { type: string }",
            "    SkillDefinition:",
            "      type: object",
            "      required: [name, description, execution_command, schema, category]",
            "      properties:",
            "        id: { type: string, nullable: true }",
            "        name: { type: string }",
            "        description: { type: string }",
            "        execution_command: { type: string }",
            "        schema: { type: object }",
            "        oversight_required: { type: boolean }",
            "        doc_url: { type: string, nullable: true }",
            "        tags:",
            "          type: array",
            "          items: { type: string }",
            "        full_instructions: { type: string, nullable: true }",
            "        negative_constraints:",
            "          type: array",
            "          items: { type: string }",
            "        verification_script: { type: string, nullable: true }",
            "        category: { type: string }",
            "    WorkflowDefinition:",
            "      type: object",
            "      required: [name, content, category]",
            "      properties:",
            "        id: { type: string, nullable: true }",
            "        name: { type: string }",
            "        content: { type: string }",
            "        doc_url: { type: string, nullable: true }",
            "        tags:",
            "          type: array",
            "          items: { type: string }",
            "        category: { type: string }",
            "    HookDefinition:",
            "      type: object",
            "      required: [name, description, hook_type, content, active, category]",
            "      properties:",
            "        name: { type: string }",
            "        description: { type: string }",
            "        hook_type: { type: string }",
            "        content: { type: string }",
            "        active: { type: boolean }",
            "        category: { type: string }",
            "    CreateJobRequest:",
            "      type: object",
            "      required: [agent_id, name, prompt, cron_expr]",
            "      properties:",
            "        agent_id: { type: string }",
            "        workflow_id: { type: string, nullable: true }",
            "        name: { type: string }",
            "        prompt: { type: string }",
            "        cron_expr: { type: string }",
            "        budget_usd: { type: number, nullable: true }",
            "        max_failures: { type: integer, nullable: true }",
            "        metadata: { type: object, nullable: true }",
            "    UpdateJobRequest:",
            "      type: object",
            "      properties:",
            "        name: { type: string, nullable: true }",
            "        prompt: { type: string, nullable: true }",
            "        workflow_id: { type: string, nullable: true }",
            "        cron_expr: { type: string, nullable: true }",
            "        budget_usd: { type: number, nullable: true }",
            "        enabled: { type: boolean, nullable: true }",
            "        max_failures: { type: integer, nullable: true }",
            "    CreateWorkflowRequest:",
            "      type: object",
            "      required: [name]",
            "      properties:",
            "        name: { type: string }",
            "        description: { type: string, nullable: true }",
            "    AddStepRequest:",
            "      type: object",
            "      required: [agent_id, name, prompt_template, step_order]",
            "      properties:",
            "        agent_id: { type: string }",
            "        name: { type: string }",
            "        prompt_template: { type: string }",
            "        step_order: { type: integer }",
            "    AppendNodeRequest:",
            "      type: object",
            "      required: [role, content]",
            "      properties:",
            "        parent_id: { type: string, nullable: true }",
            "        role: { type: string }",
            "        content: { type: string }",
            "        metadata: { type: object, nullable: true }",
            "    OversightDecision:",
            "      type: object",
            "      required: [decision]",
            "      properties:",
            "        decision: { type: string }",
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
