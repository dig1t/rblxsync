# rblxsync.yml — Configuration Schema

Parsed by `serde_yaml` into `RblxSyncConfig`. The default config path is
`rblxsync.yml`; override with the global `--config` flag. Only `universe.id` is
required.

## Root

| Field | Type | Required | Default | Notes |
| --- | --- | --- | --- | --- |
| `assets_dir` | string | No | `assets` | Directory holding icon files referenced by resources. |
| `creator` | object | No | – | Asset creation context. Required only when uploading icons. |
| `universe` | object | **Yes** | – | Target universe + settings. |
| `game_passes` | list | No | `[]` | Game passes to sync. |
| `developer_products` | list | No | `[]` | Developer products to sync. |
| `badges` | list | No | `[]` | Badges to sync (100 Robux each to create). |
| `places` | list | No | `[]` | Places available to `rblxsync publish`. |
| `badge_payment_source` | string | No | – | `"user"` or `"group"` — who pays the 100 Robux badge fee. |
| `output_path` | string | No | – | Where `run` regenerates the typed `Config.luau` (e.g. `src/shared/Config.luau`). |

## `creator`

Drives the asset `creationContext` (user vs group) for uploaded icons.

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `id` | string | **Yes** | User or group id, **as a string** (quote it). |
| `type` | string | **Yes** | `"user"` or `"group"`. Anything other than `"group"` is treated as a user. |

## `universe`

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `id` | number (u64) | **Yes** | Universe ID (NOT a place ID). |
| `name` | string | No | Experience name. **Synced** (cookie). |
| `description` | string | No | Experience description. **Synced** (cookie). |
| `genre` | string | No | **Local-only. Never sent to any API.** Not validated against any list. |
| `playable_devices` | list of string | No | Allowed: `computer`, `phone`, `tablet`, `console`, `vr`. Unknown values are filtered out. **Synced** (cookie). |
| `max_players` | number (u32) | No | **Local-only. Never PATCHed** (it's a per-place setting). |
| `private_server_cost` | special | No | See table below. **Synced** (cookie). |

> **Cookie trigger:** setting *any* of the above (even local-only `genre` /
> `max_players`) makes `rblxsync run` require `ROBLOX_COOKIE`. If you only want
> `universe.id` without a cookie, set nothing else under `universe`.

### `private_server_cost` values

| YAML value | Meaning | API effect |
| --- | --- | --- |
| `"disabled"` (case-insensitive) | Private servers off | `allowPrivateServers = false` |
| `"free"`, `0`, or `"0"` | Free private servers | `allowPrivateServers = true`, price `0` |
| positive integer (`100` or `"100"`) | Paid private servers | `allowPrivateServers = true`, price `n` |

Negative values and values `> u32::MAX` are rejected at parse time.

## `game_passes[]`

| Field | Type | Required | Default | Notes |
| --- | --- | --- | --- | --- |
| `id` | number (u64) | No | – | Stable Roblox id. When present it is the **authoritative match key** (renaming `name` is safe — PATCHed, never duplicated). Written automatically by `rblxsync import` and by `run` on create. |
| `name` | string | **Yes** | – | Match key **when no `id` is set** (unique, case-insensitive). For an id-less entry, renaming creates a new pass. |
| `description` | string | No | – | |
| `price` | number (u32) | No | `0` on create | Robux. |
| `icon` | string | No | – | Filename relative to `assets_dir`. Re-uploaded only when its SHA-256 changes. |
| `is_for_sale` | boolean | No | – | **Synced.** |

## `developer_products[]`

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `id` | number (u64) | No | Stable Roblox id. Authoritative match key when present (safe rename). Written by `import` and by `run` on create. |
| `name` | string | **Yes** | Match key when no `id` is set, unique case-insensitive. |
| `description` | string | No | |
| `price` | number (u32) | **Yes** | Robux. **Required** (unlike game passes). |
| `icon` | string | No | Filename relative to `assets_dir`. |
| `is_active` | boolean | No | **Parsed but NEVER synced.** Has no effect today. Don't rely on it. |

## `badges[]`

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `id` | number (u64) | No | Stable Roblox id. Authoritative match key when present (safe rename). Written by `import` and by `run` on create. |
| `name` | string | **Yes** | Match key when no `id` is set, unique case-insensitive. |
| `description` | string | No | |
| `icon` | string | No | Filename relative to `assets_dir`. |
| `is_enabled` | boolean | No | Mapped to the API `enabled` field on PATCH. |

Creating a badge costs **100 Robux** and needs `badge_payment_source`
(`"user"` or `"group"`). Confirm with the user before syncing new badges.
**A new badge also requires an `icon`** — Roblox rejects creation without one
(`code 11`, "The badge icon is invalid."). rblxsync fails fast with a message
naming the badge if a to-be-created badge has no valid icon. (Updating an
existing badge does not require re-supplying the icon.)

## `places[]`

| Field | Type | Required | Default | Notes |
| --- | --- | --- | --- | --- |
| `place_id` | number (u64) | **Yes** | – | Target place ID. |
| `file_path` | string | **Yes** | – | Path to a `.rbxl` / `.rbxlx` file. `rblxsync import` writes this as `""` (the binary can't be downloaded); fill it in before `publish`. |
| `publish` | boolean | No | `false` | Only `publish: true` places are published by `rblxsync publish`. |

## Validation (`rblxsync validate`)

Runs with no API key and no network. It confirms the file exists, parses it, and
**rejects case-insensitive duplicate names** within game passes, products, or
badges. Always validate before a real sync.

## Duplicate-name pitfall

Because name matching is case-insensitive, two entries like `"VIP Pass"` and
`"vip pass"` in the same list are a hard validation error. Across the live
account, a name that already exists on Roblox is updated rather than duplicated;
a name that doesn't exist is created. Setting a stable `id` on an entry sidesteps
the name trap entirely: matching is then by `id`, so the `name` can be edited
freely. `rblxsync import` fills in these ids for you.
