> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Core**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[codebase_review]` in audit logs.
>
> ### AI Assist Note
> 🗺️ Directive: Comprehensive Codebase Review (SOP-DEV-02)
>
> ### 🔍 Debugging & Observability
> Traceability via `sovereign_audit.py`.

# 🗺️ Directive: Comprehensive Codebase Review (SOP-DEV-02)

## 🎯 Primary Objective
Execute a multi-dimensional analysis of the Tadpole OS repository to evaluate performance, security, maintainability, and architectural alignment. Generate a high-fidelity "Codebase Health Report" for the sovereign architect.

---

## 🏗️ Review Dimensions

### 1. Architectural Integrity
- **Check**: Adherence to the 3-layer architecture (Directive, Orchestration, Execution).
- **Rule**: Ensure logic is not leaking from `Execution` scripts into the `Orchestration` layer.

### 2. Code Quality & Performance
- **Check**: Identification of anti-patterns, redundant loops, or blocking calls in asynchronous contexts (Axum/Tokio).
- **Rule**: Maximize use of zero-copy patterns in Rust and avoid unnecessary re-renders in React.

### 3. Security & Compliance
- **Check**: Scan for hardcoded credentials, unsafe `unsafe` blocks in Rust, and unsanitized inputs in the command processor.
- **Rule**: 100% compliance with the `Security Scan (P0)` audit pillar.

### 4. Documentation & Parity
- **Check**: Verify that every `@docs` tag points to a valid file and that `.env.example` is synchronized.
- **Rule**: Achieve 0 errors in `parity_guard.py`.

---

## 🛠️ Execution Protocol

### Step 1: Automated Baseline
Run the full sovereign audit suite to identify immediate failures:
```bash
python execution/sovereign_audit.py .
```

### Step 2: Deep Context Analysis
Manually inspect the top 5 high-complexity files (detected by `verify_ai_context.py`) for logic drift.

### Step 3: Vulnerability Assessment
Run the localized security scanner:
```bash
python execution/security_scan.py .
```

---

## 📝 Reporting Template
The agent must provide the report in the following structure:
1. **Executive Summary**: Overall health score (0-100%).
2. **Critical Vulnerabilities**: Immediate security or stability risks.
3. **Refactor Roadmap**: Prioritized list of technical debt items.
4. **Compliance Status**: Detailed pillar-by-pillar audit results.

[//]: # (Metadata: [codebase_review])
