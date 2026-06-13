---
slug: /api
sidebar_position: 5
title: API Reference
---

# rblxsync API Reference

`rblxsync` is a Rust CLI tool and GitHub Action for declaratively managing Roblox experience metadata via the Open Cloud API. It synchronizes Universe settings, Game Passes, Developer Products, Badges, and Places from a local YAML configuration file (`rblxsync.yml`).

This document is the authoritative reference for the CLI surface, the `rblxsync.yml` schema, the GitHub Action, environment variables, the generated lock file, the generated Luau output, and the Open Cloud permissions required. For installation and a getting-started walkthrough, see the [README](https://github.com/dig1t/rblxsync/blob/main/README.md).

---

## CLI Commands

The binary is named `rblxsync`. Top-level usage:

```
rblxsync [--config <PATH>] [COMMAND]
```

### Global flags

| Flag | Alias | Default | Description |
| --- | --- | --- | --- |
| `--config <PATH>` | `-c` | `rblxsync.yml` | Path to the YAML config file. Applies to every subcommand. |

If no subcommand is given, `rblxsync` defaults to `run` (with `dry_run = false`).

### `run`

```
rblxsync run [--dry-run]
```

Syncs universe settings and assets (game passes, developer products, badges) against the Open Cloud API. Idempotent: resources are matched by name (case-insensitive), created if missing, and updated (PATCH) if they exist. Icons are only re-uploaded when their local SHA-256 hash differs from the hash stored in the lock file.

| Flag | Description |
| --- | --- |
| `--dry-run` | Previews changes without applying them. Makes no mutating HTTP calls, does not save state, and does not write `Config.luau`. |

Behavior notes:

- If `universe.has_settings()` is true (any of `name`, `description`, `genre`, `playable_devices`, `max_players`, or `private_server_cost` is set), `run` **requires** the `ROBLOX_COOKIE` environment variable. If it is absent, the command prints `.ROBLOSECURITY` cookie-setup instructions and exits with status `1`.
- On success, `run` writes `rblxsync-lock.yml`.
- If `output_path` is set in the config, `run` regenerates the Luau config at that path after a successful sync.

> **Path note:** The lock file is **loaded** from the config file's parent directory but **saved** to the current working directory. These differ when the config is not in the cwd. Run `rblxsync` from the directory containing `rblxsync-lock.yml` to keep state consistent.

### `publish`

```
rblxsync publish
```

Publishes every place in the config where `publish: true`. For each such place, calls `publish_place` with the place's `file_path`. This command does **not** require `ROBLOX_COOKIE`.

- Places with a missing `file_path` are skipped (an error is logged) and processing continues.
- Per-place errors are logged but are **not** fatal; remaining places are still attempted.

### `validate`

```
rblxsync validate
```

Validates the YAML config **without** requiring `ROBLOX_API_KEY` (this check runs before environment loading). It:

1. Confirms the config file exists.
2. Parses it into `RblxSyncConfig`.
3. Rejects case-insensitive duplicate names among game passes, developer products, and badges.

Exits `1` on any failure; logs `Config file is valid.` on success.

### `export`

```
rblxsync export [--output <PATH>] [--lua]
```

Pulls live game passes, developer products, and badges from the Open Cloud API and writes them to a Luau/Lua table file.

| Flag | Alias | Description |
| --- | --- | --- |
| `--output <PATH>` | `-o` | Overrides the output file path. |
| `--lua` | | Changes only the **default** filename to `config.lua` instead of `config.luau`. The generated content is identical and valid for both. |

Default output filename: `config.luau` (or `config.lua` with `--lua`).

Exported table shape (note: this is **flatter** than the richer `Config.luau` produced by `run`):

```lua
return {
    game_passes = {
        { name = "VIP Pass", id = 123456, price = 100 },
    },
    developer_products = {
        { name = "Speed Boost", id = 234567, price = 50 },
    },
    badges = {
        { name = "First Win", id = 345678 },
    },
}
```

---

## Configuration Schema

The config file (`rblxsync.yml` by default) is parsed into `RblxSyncConfig` via `serde_yaml`.

### Root (`RblxSyncConfig`)

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `assets_dir` | string | No | `assets` | Directory containing icon files referenced by resources. |
| `creator` | object (`CreatorConfig`) | No | – | Asset creation context. Required only when uploading icons. |
| `universe` | object (`UniverseConfig`) | **Yes** | – | Target universe and its settings. |
| `game_passes` | list (`GamePassConfig`) | No | `[]` | Game passes to sync. |
| `developer_products` | list (`DeveloperProductConfig`) | No | `[]` | Developer products to sync. |
| `badges` | list (`BadgeConfig`) | No | `[]` | Badges to sync. |
| `places` | list (`PlaceConfig`) | No | `[]` | Places available to the `publish` command. |
| `badge_payment_source` | string | No | – | Payment source for badge creation (costs 100 Robux per badge). `"user"` or `"group"`. |
| `output_path` | string | No | – | Path where `run` regenerates the Luau config (e.g. `src/shared/Config.luau`). |

### `creator` (`CreatorConfig`)

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `id` | string | **Yes** | – | User or group id (as a string). |
| `type` | string | **Yes** | – | `"user"` or `"group"`. Any value other than `"group"` is treated as a user. |

### `universe` (`UniverseConfig`)

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `id` | number (u64) | **Yes** | – | Universe ID. |
| `name` | string | No | – | Experience name. |
| `description` | string | No | – | Experience description. |
| `genre` | string | No | – | Genre. Tracked in state but **never sent to any API** (not API-updatable). |
| `playable_devices` | list of string | No | – | Allowed devices: `computer`, `phone`, `tablet`, `console`, `vr` (mapped to ids 1–5). Unknown values are filtered out. |
| `max_players` | number (u32) | No | – | Tracked in state but **never sent to the universe configuration API** (max players is a per-place setting). |
| `private_server_cost` | `PrivateServerCost` | No | – | See below. |

Setting any of these fields triggers the `ROBLOX_COOKIE` requirement for `run`.

#### `private_server_cost` values

| YAML value | Meaning | API effect |
| --- | --- | --- |
| `"disabled"` (case-insensitive) | Private servers off | `allowPrivateServers = false` |
| `"free"`, `0`, or `"0"` | Free private servers | `allowPrivateServers = true`, `privateServerPrice = 0` |
| positive integer (or quoted, e.g. `100` / `"100"`) | Paid private servers | `allowPrivateServers = true`, `privateServerPrice = n` |

Negative values and values greater than `u32::MAX` are rejected at parse time.

### `game_passes[]` (`GamePassConfig`)

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `name` | string | **Yes** | – | Unique (case-insensitive). Used as the match key. |
| `description` | string | No | – | Game pass description. |
| `price` | number (u32) | No | `0` on create | Price in Robux. |
| `icon` | string | No | – | Icon filename relative to `assets_dir`. |
| `is_for_sale` | boolean | No | – | Whether the pass is for sale. |

### `developer_products[]` (`DeveloperProductConfig`)

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `name` | string | **Yes** | – | Unique (case-insensitive). |
| `description` | string | No | – | Product description. |
| `price` | number (u32) | **Yes** | – | Price in Robux. Unlike game passes, this is required. |
| `icon` | string | No | – | Icon filename relative to `assets_dir`. |
| `is_active` | boolean | No | – | **Parsed but never synced.** Currently has no effect. |

### `badges[]` (`BadgeConfig`)

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `name` | string | **Yes** | – | Unique (case-insensitive). |
| `description` | string | No | – | Badge description. |
| `icon` | string | No | – | Icon filename relative to `assets_dir`. |
| `is_enabled` | boolean | No | – | Mapped to the API field `enabled` on PATCH. |

Creating a badge costs 100 Robux and requires `badge_payment_source`. If it is missing, the API error triggers a helpful message.

### `places[]` (`PlaceConfig`)

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `place_id` | number (u64) | **Yes** | – | Target place ID. |
| `file_path` | string | **Yes** | – | Path to a `.rbxl` / `.rbxlx` file. |
| `publish` | boolean | No | `false` | Only places with `publish: true` are published by `rblxsync publish`. |

### Example `rblxsync.yml`

```yaml
assets_dir: assets/icons
badge_payment_source: "user"
output_path: "src/shared/Config.luau"

creator:
  id: "12345678"
  type: "user"

universe:
  id: 123456789
  name: "My Awesome Game"
  description: "Updated via rblxsync!"
  genre: "adventure"
  playable_devices: ["computer", "phone"]
  max_players: 50
  private_server_cost: "disabled"

game_passes:
  - name: "VIP Pass"
    description: "Unlocks VIP perks"
    price: 100
    icon: "vip_pass.png"
    is_for_sale: true

developer_products:
  - name: "Speed Boost"
    description: "Temporary speed boost"
    price: 50
    icon: "speed_boost.png"

badges:
  - name: "First Win"
    description: "Awarded for your first victory"
    icon: "first_win.png"
    is_enabled: true

places:
  - place_id: 1234567890
    file_path: "places/start_place.rbxl"
    publish: true
```

---

## GitHub Action

`rblxsync` ships as a composite GitHub Action. It checks out the tool into `.rblxsync-action`, sets up stable Rust, caches cargo, builds `--release`, and runs `rblxsync "$COMMAND" --config "$CONFIG" $ARGS`.

### Inputs

| Input | Required | Default | Description |
| --- | --- | --- | --- |
| `api_key` | **Yes** | – | Open Cloud API key. Exposed as `ROBLOX_API_KEY`. |
| `command` | No | `run` | One of `run`, `publish`, `validate`, `export`. |
| `config` | No | `rblxsync.yml` | Path to the config file. Passed as `--config`. |
| `args` | No | `""` | Extra arguments appended **unquoted**, so multiple flags word-split (e.g. `--dry-run`). |
| `roblox_cookie` | No | `""` | `.ROBLOSECURITY` cookie. Exposed as `ROBLOX_COOKIE`. Required only for universe settings. |

### Example workflow

```yaml
name: Sync Roblox metadata

on:
  push:
    branches: [main]

jobs:
  sync:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Sync universe settings and assets
        uses: dig1t/rblxsync@main
        with:
          api_key: ${{ secrets.ROBLOX_API_KEY }}
          roblox_cookie: ${{ secrets.ROBLOX_COOKIE }}
          command: run
          config: rblxsync.yml
          args: "--dry-run"
```

---

## Environment Variables

| Variable | Required | Description |
| --- | --- | --- |
| `ROBLOX_API_KEY` | Yes (all commands except `validate`) | Open Cloud API key. Sent as the `x-api-key` header. `validate` does not need it. |
| `ROBLOX_COOKIE` | Conditional | `.ROBLOSECURITY` cookie. Required **only** when universe settings are defined, for the `develop.roblox.com` configuration PATCH. |
| `RUST_LOG` | No | Standard `env_logger` filter. Defaults to `info`. |

Both `ROBLOX_API_KEY` and `ROBLOX_COOKIE` are loaded from `.env` via `dotenvy`. `.env` is gitignored; never commit or print these values.

---

## Lock File (`rblxsync-lock.yml`)

`rblxsync-lock.yml` is generated local state. **Do not hand-edit it.** It tracks resource IDs and local icon hashes so updates stay idempotent (only changed icons are re-uploaded). A missing file is treated as empty default state.

Top-level keys:

| Key | Type | Description |
| --- | --- | --- |
| `universe` | `UniverseState` (optional) | Omitted if no universe settings are tracked. |
| `game_passes` | map of `u64` resource id → `ResourceState` | |
| `developer_products` | map of `u64` resource id → `ResourceState` | |
| `badges` | map of `u64` resource id → `ResourceState` | |

`UniverseState` fields: `name?`, `description?`, `genre?`, `playable_devices?` (`{string}`), `max_players?` (number), `private_server_cost?` (string: `disabled` | `0` | numeric string).

`ResourceState` fields: `name` (string), `description?`, `price?` (u64), `is_for_sale?` (bool, game passes), `is_enabled?` (bool, badges), `icon_hash?` (SHA-256 hex of the local icon for change detection), `icon_asset_id?` (u64 uploaded image asset id; badges do not store this).

---

## Generated Luau Output (`Config.luau`)

When `output_path` is set, `run` regenerates a strict-typed Luau module (`--!strict`) at that path after a successful sync. Output is deterministic (resources sorted by id). Game code can `require` this module to read resource IDs and metadata. Strings are escaped for backslash, `"`, `\n`, `\r`, and `\t`.

### Shape

```lua
--!strict
-- Auto-generated by rblxsync. Do not edit manually.
-- This file is regenerated each time `rblxsync run` completes.

export type Universe = {
    Id: number,
    Name: string?,
    Description: string?,
    Genre: string?,
    PlayableDevices: {string}?,
    MaxPlayers: number?,
    PrivateServerCost: (number | "disabled")?,
}

export type GamePass = {
    Id: number,
    Name: string,
    Description: string?,
    Price: number?,
    IsForSale: boolean?,
}

export type DeveloperProduct = {
    Id: number,
    Name: string,
    Description: string?,
    Price: number?,
}

export type Badge = {
    Id: number,
    Name: string,
    Description: string?,
    IsEnabled: boolean?,
}

return {
    Universe = {
        Id = 123456789,
        Name = "My Awesome Game",
        Description = "Updated via rblxsync!",
        Genre = "adventure",
        PlayableDevices = { "computer", "phone" },
        MaxPlayers = 50,
        PrivateServerCost = "disabled",
    } :: Universe,

    GamePasses = {
        {
            Id = 123456,
            Name = "VIP Pass",
            Description = "Unlocks VIP perks",
            Price = 100,
            IsForSale = true,
        },
    } :: { GamePass },

    DeveloperProducts = {
        {
            Id = 234567,
            Name = "Speed Boost",
            Price = 50,
        },
    } :: { DeveloperProduct },

    Badges = {
        {
            Id = 345678,
            Name = "First Win",
            IsEnabled = true,
        },
    } :: { Badge },
}
```

`PrivateServerCost` is emitted as a bare number for numeric costs and as the string `"disabled"` when disabled.

### Consuming it in-game

```lua
local Config = require(game.ReplicatedStorage.Shared.Config)

local gamePassId = Config.GamePasses[1].Id
print(Config.Universe.Name)
```

> This `Config.luau` shape (generated by `run`) is richer than the flat table produced by `export`. Do not confuse the two.

---

## Open Cloud Permissions

Universe **settings** updates do **not** use the API key at all. They require the `.ROBLOSECURITY` cookie because `develop.roblox.com/v2/.../configuration` is not an Open Cloud key endpoint. The `creator` `id`/`type` in config drives the asset `creationContext` (user vs group).

The Open Cloud API key (`ROBLOX_API_KEY`) needs the following scopes:

| Feature | Scope | Endpoints / notes |
| --- | --- | --- |
| Game Passes | read + write | `GET`/`POST`/`PATCH .../game-passes/v1/universes/{uid}/game-passes` |
| Developer Products | read + write | `GET`/`POST`/`PATCH .../developer-products/v2/universes/{uid}/developer-products` |
| Badges | read + create/manage | List via `badges.roblox.com/v1/universes/{uid}/badges`; create/update/icon via the legacy `legacy-badges` / `legacy-publish` endpoints |
| Assets (icons) | upload | `POST /assets/v1/assets` (multipart), polled at `GET /assets/v1/{operation}` |
| Places | publish (versions write) | `POST /v1/universes/{uid}/places/{placeId}/versions?versionType=Published` |

`429` responses are retried up to 3 times honoring `Retry-After`.
