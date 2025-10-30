# Repository Guidelines

## Project Structure & Module Organization
The service is a single Cargo crate (`Cargo.toml`) targeting Rust 2021. Runtime entry sits in `src/main.rs`, which orchestrates config loading (`src/config.rs`), error mapping (`src/error.rs`), and router setup. Authentication concerns live under `src/auth/` (handlers, JWT, middleware). `src/proxy/` owns request queueing and rate limiting, while `src/deepseek/` wraps outbound GLM client logic. Quota persistence is isolated in `src/quota/`. Runtime configuration sits in `config.toml`; environment overrides live in `.env`. Quota snapshots default to `data/quotas/*.json`, and build artifacts land in `target/`.

## Build, Test, and Development Commands
Use Cargo for day-to-day workflows:
```bash
cargo run                   # start the proxy with hot recompile-style workflow
cargo build --release       # emit optimized binary at target/release/deepseek_proxy
cargo fmt && cargo clippy --all-targets -- -D warnings
```
PowerShell helpers (`build-wsl.ps1`, `build-linux.ps1`) package cross-compilation presets; keep them in sync with dependency updates.

## Coding Style & Naming Conventions
Follow idiomatic Rust formatting enforced via `cargo fmt`. Modules and functions stay in `snake_case`, types and traits in `PascalCase`, and constants in `SCREAMING_SNAKE_CASE`. Prefer `anyhow::Result` for fallible operations at the edges and `thiserror` enums for domain errors. Log through `tracing` macros with structured fields where possible.

## Testing Guidelines
Unit and integration coverage should live alongside code and run with `cargo test`. For end-to-end validation, keep one terminal running `cargo run` and execute:
```bash
python test_proxy.py        # exercises auth, streaming, and quota paths
python test_deepseek.py     # validates direct DeepSeek passthrough
```
Refresh `data/quotas/*.json` fixtures when changing quota logic, and document manual scenarios in commit notes.

## Commit & Pull Request Guidelines
Match the existing history: concise, task-oriented subject lines (Chinese or English) that call out the affected area, e.g., “修复 quota 超限处理”. Squash noisy intermediate commits before pushing. Pull requests should state the problem, summarize the solution, list config or schema changes, and include the exact commands/tests you ran. Attach logs or screenshots for UI- or script-facing changes.

## Security & Configuration Tips
Never commit real API keys or JWT secrets; derive them from `.env` or deployment-specific `config.toml`. When sharing configs, redact sensitive values and highlight required overrides such as `GLM_FLASH_API_KEY`, `auth.users`, and `jwt_secret`. Rotate tokens regularly and update rate-limit settings to match upstream quotas.
