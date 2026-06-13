---
slug: /
sidebar_position: 1
title: Introduction
---

# rblxsync

`rblxsync` is a Rust-based CLI tool and GitHub Action for declaratively managing Roblox experience metadata via the Open Cloud API. Define your Universe settings, Game Passes, Developer Products, Badges, and Places in a single YAML file (`rblxsync.yml`) and sync them to Roblox with one command.

Full reference documentation lives in the [API reference](/api): CLI commands, the configuration schema, GitHub Action inputs, environment variables, the lock file, generated Luau output, and Open Cloud permissions.

## Features

- **Declarative configuration**: manage all game metadata in `rblxsync.yml`.
- **Idempotent sync**: resources are matched by name (case-insensitive); created if missing, updated if present.
- **Icon management**: icons for Game Passes, Products, and Badges are re-uploaded only when the local file changes (SHA-256 checksum).
- **Place publishing**: publish `.rbxl` files to specific Place IDs.
- **Export**: dump existing Roblox resources to a flat Luau/Lua table (a one-way snapshot, not a config you can feed back in).
- **Auto-generated config**: write a typed Luau module (`output_path`) containing all resource IDs after each sync.
- **CI/CD ready**: ships as a GitHub Action.

## What rblxsync does

The source of truth is `rblxsync.yml` in your project root. A minimal config needs only `universe.id`. Top-level sections:

| Section | Purpose |
|---------|---------|
| `universe` | Universe ID and settings (name, description, genre, devices, max players, private server cost). **Required** (`universe.id`). |
| `creator` | User/group that owns uploaded asset icons. Required only when uploading icons. |
| `assets_dir` | Directory for icon files (default `"assets"`). |
| `game_passes` | Game Pass definitions (matched by name). |
| `developer_products` | Developer Product definitions (matched by name). |
| `badges` | Badge definitions (matched by name). Creating a badge costs **100 Robux** each. |
| `places` | Places to publish via `rblxsync publish`. |
| `badge_payment_source` | `"user"` or `"group"`: who pays the badge creation fee. |
| `output_path` | Path for the auto-generated typed Luau module. |

A few behaviors worth knowing up front:

- **`genre` and `max_players` are local-only.** They are written to the lock file and the generated `Config.luau`, but are **never** pushed to Roblox by this tool. The `genre` value is not validated against any list. `max_players` is a per-place setting and is not PATCHed.
- **Game Pass `is_for_sale` IS synced.** Developer Product `is_active` is parsed but **not** synced (it has no effect today).
- **`private_server_cost`** accepts `"disabled"`, `0` (free), or a Robux amount (e.g. `100`). Quoted (`"0"`) and unquoted (`0`) numbers both work; this doc uses unquoted numbers and the string `"disabled"`.

See the complete, field-by-field schema in the [configuration schema](/api#configuration-schema).
