# AGENTS.md - rbxsync

This file guides AI agents working on the `rbxsync` project. It serves as context for understanding the codebase, conventions, and development workflow.

## Project Overview
`rbxsync` is a Rust-based CLI tool and GitHub Action for interacting with the Roblox Open Cloud API. It handles authentication, request signing, and data synchronization.

## Tech Stack
- **Language**: Rust (2021 edition)
- **CLI Framework**: `clap` (derive feature)
- **HTTP Client**: `reqwest` (async, json, rustls)
- **Async Runtime**: `tokio`
- **Serialization**: `serde` & `serde_json`
- **Error Handling**: `anyhow` for application-level errors.
- **Logging**: `log` & `env_logger`.

## Directory Structure
- `src/main.rs`: CLI entry point, command parsing, and high-level execution flow.
- `src/api/mod.rs`: `RobloxClient` implementation. Encapsulates all HTTP interaction logic.
- `src/config.rs`: Configuration loading logic (env vars and `.env`).
- `action.yml`: GitHub Action metadata for using this tool in CI workflows.

## Development Guidelines

### Adding New Commands
1.  **Define Subcommand**: Add a variant to the `Commands` enum in `src/main.rs`.
2.  **Implement Logic**: Create a new async method in `RobloxClient` (`src/api/mod.rs`) that handles the API interaction.
3.  **Wire Up**: Match the new subcommand in `main` and call the client method.
4.  **Logging**: Use `info!` for progress and `error!` for failures.

### Adding New API Endpoints
- Extend `RobloxClient` in `src/api/mod.rs`.
- Use the provided `self.get` or `self.post` helper methods. These automatically handle:
    - Base URL prepending (`https://apis.roblox.com`)
    - API Key injection (`x-api-key` header)
    - Status code validation (converts non-200s to errors)
- Create strongly typed structs for request/response bodies using `serde` when possible. Fallback to `serde_json::Value` only for untyped or dynamic data.

### Error Handling
- Use `anyhow::Result` for most return types to simplify error propagation.
- Add context to errors using `.context("message")` when bubbling up.
- Avoid `.unwrap()` in production code; use `?` or `unwrap_or_else`.

### Style & Linting
- Follow standard Rust formatting (`cargo fmt`).
- Ensure `cargo clippy` passes without warnings.
- Prefer `cloned()` over `clone()` for iterators/options where applicable.

## Environment Variables
The application relies on `Config::from_env()` in `src/config.rs`:
- `ROBLOX_API_KEY`: **Required**. The Open Cloud API Key.
- `ROBLOX_UNIVERSE_ID`: Required for universe-specific operations (e.g., Datastores, Messaging Service).

## Testing
- **Unit Tests**: `cargo test`
- **Manual CLI Test**: `cargo run -- list-datastores --limit 1` (Requires `.env` file with valid credentials).

