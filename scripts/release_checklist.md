# Release Checklist

This guide ensures release builds are clean, verified, and complete.

## 1. Pre-Flight Compilation & Gates

- [ ] Ensure all local changes are committed.
- [ ] Run `python execution/verify_all.py` and confirm 100% test success.
- [ ] Run `python execution/verify_ai_context.py` to check context alignment.
- [ ] Validate environment variables against schema using `npm run version:sync`.

## 2. Version Bump & Sync

- [ ] Bump version in `version.json` (e.g. `1.1.58`).
- [ ] Run version synchronization:
  ```bash
  python execution/sync_version.py
  ```
  This automatically propagates the new version string to `Cargo.toml`, `package.json`, `package-lock.json`, and all manifest defaults.

## 3. SBOM & Dependency Auditing

- [ ] Generate the Software Bill of Materials (SBOM) for the Rust binary:
  ```bash
  cargo syft target/release/server-rs.exe -o spdx-json > sbom-rust.json
  ```
- [ ] Generate the SBOM for the node frontend:
  ```bash
  npm run sbom
  ```

## 4. Release Build

- [ ] Build release packages:
  ```powershell
  powershell -ExecutionPolicy Bypass -File scripts/deploy-linuxlite.ps1
  ```
