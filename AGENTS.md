# AGENTS.md - rbxsync

This file guides AI agents working on the `rbxsync` project. It serves as context for understanding the codebase, conventions, and development workflow.

## Project Overview
`rbxsync` is a Rust-based CLI tool and GitHub Action for **declaratively managing** Roblox experience metadata via the Open Cloud API. It supports synchronizing Universe settings, Game Passes, Developer Products, Badges, and Places from a local YAML configuration file (`rbxsync.yml`).

## Tech Stack
- **Language**: Rust (2021 edition)
- **CLI Framework**: `clap` (derive feature)
- **HTTP Client**: `reqwest` (async, json, multipart, rustls)
- **Async Runtime**: `tokio`
- **Serialization**: `serde`, `serde_json`, `serde_yaml`
- **Hashing**: `sha2` (for icon change detection)
- **Error Handling**: `anyhow` for application-level errors.
- **Logging**: `log` & `env_logger`.

## Directory Structure
- `src/main.rs`: CLI entry point. Handles arguments, loads env vars, and dispatches commands (`Run`, `Publish`, `Export`, `Validate`).
- `src/api/mod.rs`: `RobloxClient` implementation. Encapsulates all Open Cloud API interactions (PATCH, POST, GET, Multipart Uploads).
- `src/config.rs`: 
    - `Config`: Loads environment variables (`ROBLOX_API_KEY`).
    - `RbxSyncConfig`: Structs for parsing `rbxsync.yml` configuration.
- `src/state.rs`: Manages `rbxsync-lock.yml`. Tracks resource IDs and local icon hashes for idempotent updates.
- `src/commands.rs`: Core business logic for `run`, `publish`, and `export` commands.
- `action.yml`: GitHub Action metadata.

## Development Guidelines

### Workflow
1.  **Configuration Driven**: The source of truth is `rbxsync.yml`.
2.  **Idempotency**: Operations should be idempotent.
    - Match resources by **name** (case-sensitive).
    - Create if missing.
    - Update if exists (PATCH).
    - **Icons**: Calculate SHA-256 of local file. Compare with stored hash in `rbxsync-lock.yml`. Only upload if changed.

### CLI Commands
- `rbxsync run`: Syncs universe settings + assets (Game Passes, Products, Badges).
- `rbxsync publish`: Publishes places defined in config.
- `rbxsync export`: Pulls existing data and generates a Luau/Lua config.
- `rbxsync validate`: Validates the YAML config format.

### API Integration (`src/api/mod.rs`)
- **Universe**: `PATCH .../configuration`
- **Game Passes/Products**: Standard GET/POST/PATCH flow.
- **Assets**: `POST .../assets` (Multipart). Requires polling the operation for `assetId`.
- **Places**: `POST .../versions` (Binary body).

### Adding New Features
1.  **Update Config**: Add fields to `RbxSyncConfig` in `src/config.rs`.
2.  **Update State**: Add tracking fields to `SyncState` in `src/state.rs` if ID/Hash persistence is needed.
3.  **Implement Logic**: Add logic to `src/commands.rs`.
4.  **API Support**: Add methods to `RobloxClient` in `src/api/mod.rs`.

### Error Handling
- Use `anyhow::Result` for return types.
- Contextualize errors: `.context("Failed to upload icon")?`.

## Environment Variables
- `ROBLOX_API_KEY`: **Required**. Open Cloud API Key with permissions for Universe, Game Passes, Badges, Products, Assets, and Places.

## Configuration
- `universe.id`: **Required** in `rbxsync.yml`. The target Universe ID.

## Testing
- **Manual Sync**: `cargo run -- run --dry-run` (Note: dry-run logic may be partial).
- **Publish Test**: `cargo run -- publish` (Ensure `publish: true` in config).

## Roblox API and Docs
Refer to these links for official roblox documentation.

- https://raw.githubusercontent.com/Roblox/creator-docs/refs/heads/main/content/en-us/reference/cloud/openapi.json
- https://create.roblox.com/docs/cloud/reference/openapi
- https://github.com/Roblox/creator-docs/blob/main/content/en-us/cloud/reference/errors.md
- https://github.com/Roblox/creator-docs/blob/main/content/en-us/cloud/guides/secrets-store.md
- https://github.com/Roblox/creator-docs/blob/main/content/en-us/cloud/guides/usage-assets.md
- https://github.com/Roblox/creator-docs/blob/main/content/en-us/cloud/guides/usage-place-publishing.md