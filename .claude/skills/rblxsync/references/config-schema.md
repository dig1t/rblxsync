# rblxsync.yml: Configuration Schema

Parsed by `serde_yaml` into `RblxSyncConfig`. The default config path is
`rblxsync.yml`; override with the global `--config` flag. Only `universe.id` is
required.

## Root

| Field | Type | Required | Default | Notes |
| --- | --- | --- | --- | --- |
| `assets_dir` | string | No | `assets` | Directory holding icon and thumbnail files the config references. |
| `creator` | object | No | – | Asset creation context. Required only when uploading icons. |
| `universe` | object | **Yes** | – | Target universe + settings. |
| `game_passes` | list | No | `[]` | Game passes to sync. |
| `developer_products` | list | No | `[]` | Developer products to sync. |
| `badges` | list | No | `[]` | Badges to sync (5 free a day per game, then 100 Robux each to create). |
| `places` | list | No | `[]` | Place settings (`run`) and files (`rblxsync publish`). |
| `thumbnails` | object | No | – | Game page and home page thumbnails. |
| `environments` | map | No | – | Other copies of the game (dev, staging) by name. See below. |
| `badge_payment_source` | string | No | – | `"user"` or `"group"`. Needed to create badges: the wallet a badge past the free daily quota is paid from. |
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
| `name` | string | **Yes** | – | Match key when no `id` is set, unique case-insensitive. With an `id`, just a label. |
| `id` | number (u64) | No | – | Roblox resource id. Written automatically on create or name match. When set, matching uses it instead of the name, so renaming is safe. |
| `ids` | map of env name to id | No | – | The same resource's id in each environment, e.g. `{ dev: 2222222 }`. `--env` uses it instead of `id` and writes it the same way. |
| `description` | string | No | – | |
| `price` | number (u32) | No | `0` on create | Robux. |
| `icon` | string | No | – | Filename relative to `assets_dir`. Sent with the create; re-uploaded only when its SHA-256 changes. |
| `is_for_sale` | boolean | No | – | **Synced.** Left out, a new pass with a `price` is created on sale. |

## `developer_products[]`

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `name` | string | **Yes** | Match key when no `id` is set, unique case-insensitive. With an `id`, just a label. |
| `id` | number (u64) | No | Roblox resource id. Written automatically on create or name match. When set, matching uses it instead of the name, so renaming is safe. |
| `ids` | map of env name to id | No | Its id in each environment, as for game passes. |
| `description` | string | No | |
| `price` | number (u32) | **Yes** | Robux. **Required** (unlike game passes). |
| `icon` | string | No | Filename relative to `assets_dir`. |
| `is_active` | boolean | No | **Parsed but NEVER synced.** Has no effect today. Don't rely on it. |

## `badges[]`

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `name` | string | **Yes** | Match key when no `id` is set, unique case-insensitive. With an `id`, just a label. |
| `id` | number (u64) | No | Roblox resource id. Written automatically on create or name match. When set, matching uses it instead of the name, so renaming is safe. |
| `ids` | map of env name to id | No | Its id in each environment, as for game passes. |
| `description` | string | No | |
| `icon` | string | No | Filename relative to `assets_dir`. |
| `is_enabled` | boolean | No | Mapped to the API `enabled` field on PATCH. |

Creating a badge needs `badge_payment_source` (`"user"` or `"group"`). Each
game gets **5 free badges a day** (GMT); after that a badge costs **100 Robux**.
`run` reads and prints the day's free quota first and stops before a paid badge
unless given `--allow-paid-badges`. Each create sends the expected cost, so
Roblox refuses rather than charge a different amount. Confirm with the user
before syncing new badges, and before ever passing `--allow-paid-badges`.

## `places[]`

| Field | Type | Required | Default | Notes |
| --- | --- | --- | --- | --- |
| `place_id` | number (u64) | No | – | The place in the top-level universe. A place needs this or `ids`. |
| `ids` | map of env name to id | No | – | The same place in each environment. |
| `name` | string | No | – | Place name. **Synced** (API key). |
| `description` | string | No | – | Place description. **Synced** (API key). |
| `server_size` | number (u32) | No | – | Max players per server. **Synced** (API key). |
| `file_path` | string | No | – | Path to a `.rbxl` / `.rbxlx` file. Required with `publish: true`. |
| `publish` | boolean | No | `false` | Only `publish: true` places are published by `rblxsync publish`. |

`run` reads each place and PATCHes only the fields that differ (Roblox answers
409 while the place is open in Team Create). Places can't be created, so one
with no id in the universe being synced is skipped. The root place's name and
description ARE the experience's: set them under `universe:` (cookie) or on the
root place (API key only), never both; preflight stops the run if both are set.

## `environments`

A map from an environment name (letters, digits, `-`, `_`) to the top-level
keys that differ for that copy of the game. `--env <name>` deep-merges it over
the top level: mappings merge key by key, anything else replaces, and `null`
clears an inherited field so it isn't managed there.

```yaml
environments:
  dev:
    universe:
      id: 987654321      # required, must differ from the top level and other envs
      name: null         # don't rename the dev copy to the live name
    output_path: src/shared/ConfigDev.luau
```

- It can't list `game_passes`, `developer_products`, `badges`, `places`: entries
  are shared, each with its env id under `ids:`.
- Universe settings inherit. A top-level `universe.name` renames the dev copy
  (and needs the cookie) unless the env sets `name` or `null`.
- `name_prefix: "[dev] "` (environments only) prefixes the experience name and
  every place name in that copy. Pass/product/badge names never get it: game
  code looks those up by name.
- Lock file per env: `rblxsync-lock.<name>.yml` beside the config.
- Without its own `output_path`, an env writes the same `Config.luau`, which
  suits a build that syncs one copy right before uploading it.

## `thumbnails`

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `game_page` | list | No | Game page carousel, **in list order**. At most 10. |
| `home_page` | list | No | Home page tiles. At most 5. Two or more turn on thumbnail personalization. |

Each entry:

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `image` | string | **Yes** | File relative to `assets_dir` (`.png`, `.jpg`, `.gif`, `.tga`, `.bmp`). Re-uploaded only when its SHA-256 changes. |
| `alt_text` | string | No | **`game_page` only** (a validation error on `home_page`). Synced when set; left out, Roblox's alt text is kept. |

- A changed file or a removed entry deletes the upload rblxsync made for it.
  Thumbnails rblxsync didn't upload are never deleted; on the game page they stay
  after yours and **count toward the 10**, which `run` checks before changing
  anything.
- `home_page` updates the active personalization in place (statistics kept), or
  creates one when none is running.
- A missing or empty list leaves that set alone on Roblox.

## Validation (`rblxsync validate`)

Runs with no API key and no network. It confirms the file exists, parses it, and
**rejects case-insensitive duplicate names** within game passes, products, or
badges, more than 10 `game_page` or 5 `home_page` thumbnails, and `alt_text` on
a `home_page` thumbnail. It also checks every environment (own `universe.id`, no
entry lists), that every `ids` key is an environment, and that places have an id
and a `file_path` when `publish: true`. Always validate before a real sync.

## Duplicate-name pitfall

Name matching (used only when an entry has no `id`) is case-insensitive, so two
entries like `"VIP Pass"` and `"vip pass"` in the same list are a hard
validation error. Across the live account, a name that already exists on Roblox
is updated rather than duplicated; a name that doesn't exist is created, and the
new `id` is written back into `rblxsync.yml` immediately. An entry with an `id`
matches by id alone.
