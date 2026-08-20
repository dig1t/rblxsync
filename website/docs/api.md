---
slug: /api
sidebar_position: 6
title: API Reference
---

# rblxsync reference

Every command, flag, config field, and file format in one place. If you just want to sync your first game pass, start with the [quick start](/quick-start) instead.

`rblxsync` reads a YAML file and makes your Roblox experience match it: universe settings, game passes, developer products, badges, and places.

---

## CLI commands

```
rblxsync [--config <PATH>] [COMMAND]
```

### Global flag

| Flag | Alias | Default | What it does |
| --- | --- | --- | --- |
| `--config <PATH>` | `-c` | `rblxsync.yml` | Which config file to read. |

It's a global flag, so it works on either side of the subcommand. `rblxsync run --config x.yml` and `rblxsync --config x.yml run` do the same thing.

Type `rblxsync` with no subcommand and you get `run` without `--dry-run`.

### `run`

```
rblxsync run [--dry-run]
```

Makes Roblox match your config. Safe to run as many times as you like.

| Flag | What it does |
| --- | --- |
| `--dry-run` | Prints the plan and stops. No writes, no state saved, no `Config.luau` written. |

**How each resource is matched**

1. The entry has an `id:` → that ID wins. The name is only a label, so renaming is safe and no duplicate is ever created.
2. No `id:` → matched by name, ignoring capitals.
3. No match at all → created, and the new `id:` is written straight back into your `rblxsync.yml`.

That write-back is a surgical line insert. Your comments and formatting survive it, and it happens the instant the resource is created rather than at the end of the run. If a later step fails, the ID is already saved and the next run adopts it instead of making a second copy.

**Icons** are only re-uploaded when the SHA-256 of the local file differs from the hash in the lock file.

**Other behavior**

- If any universe setting is present (`name`, `description`, `genre`, `playable_devices`, `max_players`, or `private_server_cost`), `run` requires `ROBLOX_COOKIE`. Without it, the command prints cookie setup instructions and exits `1`.
- A successful run writes `rblxsync-lock.yml`.
- If `output_path` is set, the Luau module is regenerated after the sync.

**Path warning:** the lock file is *loaded* from the config file's folder but *saved* to whatever folder you ran the command from. Those being different will desync your state. Run rblxsync from the folder holding `rblxsync-lock.yml`.

### `publish`

```
rblxsync publish
```

Uploads and publishes every place in the config with `publish: true`. Does not need `ROBLOX_COOKIE`.

- A place with a missing `file_path` is logged as an error and skipped.
- One place failing does not stop the others.

Places are always published live. There's no "save without publishing" option.

### `import`

```
rblxsync import [--universe-id <id>] [--place-id <id>]... [--badge-id <id>]...
```

Pulls an existing experience down into `rblxsync.yml` and `rblxsync-lock.yml`. Use it to adopt rblxsync on a game that already has passes, products, and badges, or to absorb a resource rblxsync doesn't know about yet.

| Flag | Repeatable | What it does |
| --- | --- | --- |
| `--universe-id <id>` | No | Which universe to import. Falls back to `universe.id` in an existing config. |
| `--place-id <id>` | Yes | An extra place to import. The API key only auto-discovers the root place, so pass one flag per additional place. |
| `--badge-id <id>` | Yes | A badge to import by ID. Roblox's badge list leaves out **disabled** badges, so this is the only way to pull those in. |

**Remote wins.** Where a value exists both locally and on Roblox, the Roblox value overwrites yours. Entries that only exist locally are kept. Lock file entries for resources that no longer exist are dropped.

Before writing, your existing `rblxsync.yml` is renamed to `rblxsync.old.yml`, then `rblxsync.old1.yml`, `.old2.yml` and so on if that name is taken. Nothing is overwritten without a backup.

Icons are not imported.

### `validate`

```
rblxsync validate
```

Checks the config without contacting Roblox. It does **not** need `ROBLOX_API_KEY`.

1. The config file exists.
2. It parses as valid YAML in the expected shape.
3. No two game passes, developer products, or badges share a name (ignoring capitals).

Exits `1` on failure, logs `Config file is valid.` on success.

### `export`

```
rblxsync export [--output <PATH>] [--lua]
```

Fetches your live game passes, developer products, and badges and dumps them into a flat Lua table.

| Flag | Alias | What it does |
| --- | --- | --- |
| `--output <PATH>` | `-o` | Where to write the file. |
| `--lua` | | Changes the **default filename** to `config.lua` instead of `config.luau`. The file contents are identical either way. |

What you get:

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

This is a one-way snapshot for reading. It is not the typed module `output_path` generates, and it is not a config you can feed back into `run`. For that, use [`import`](#import).

---

## Config file

The config is `rblxsync.yml` unless `--config` says otherwise.

### Top level

| Field | Type | Required | Default | What it is |
| --- | --- | --- | --- | --- |
| `universe` | object | **Yes** | | Which universe, plus its settings. |
| `assets_dir` | string | No | `assets` | Folder holding the icon files your resources reference. |
| `creator` | object | No | | Who owns uploaded icons. Only needed when uploading icons. |
| `game_passes` | list | No | `[]` | Game passes to sync. |
| `developer_products` | list | No | `[]` | Developer products to sync. |
| `badges` | list | No | `[]` | Badges to sync. |
| `places` | list | No | `[]` | Places available to `publish`. |
| `badge_payment_source` | string | No | | `"user"` or `"group"`. Who pays the 100 Robux per new badge. |
| `output_path` | string | No | | Where `run` writes the typed Luau module, e.g. `src/shared/Config.luau`. |

### `creator`

| Field | Type | Required | What it is |
| --- | --- | --- | --- |
| `id` | string | **Yes** | User or group ID, written as a string. |
| `type` | string | **Yes** | `"user"` or `"group"`. Anything other than `"group"` counts as a user. |

### `universe`

| Field | Type | Required | What it is |
| --- | --- | --- | --- |
| `id` | number | **Yes** | Universe ID. |
| `name` | string | No | Experience name. |
| `description` | string | No | Experience description. |
| `genre` | string | No | Saved locally only. Never sent to Roblox, never checked against a list of valid genres. |
| `playable_devices` | list of string | No | Any of `computer`, `phone`, `tablet`, `console`, `vr`. Unrecognized values are dropped silently. |
| `max_players` | number | No | Saved locally only. Never sent to Roblox, because max players is a per-place setting. |
| `private_server_cost` | see below | No | Private server pricing. |

Setting any field here other than `id` makes `run` demand `ROBLOX_COOKIE`, including the two local-only fields.

#### `private_server_cost` values

| You write | Meaning | What Roblox gets |
| --- | --- | --- |
| `"disabled"` (any capitalization) | Private servers off | `allowPrivateServers = false` |
| `"free"`, `0`, or `"0"` | Free private servers | `allowPrivateServers = true`, `privateServerPrice = 0` |
| `100` or `"100"` | Costs 100 Robux | `allowPrivateServers = true`, `privateServerPrice = 100` |

Negative numbers and anything above `u32::MAX` are rejected when the file is parsed.

### `game_passes[]`

| Field | Type | Required | What it is |
| --- | --- | --- | --- |
| `id` | number | No | Roblox game pass ID. Set it and matching uses it instead of the name. Usually written for you on create. |
| `name` | string | **Yes** | Unique, ignoring capitals. Used as the match key when there's no `id`. |
| `description` | string | No | Shown on the pass. |
| `price` | number | No | Robux. Defaults to `0` when created. |
| `icon` | string | No | Filename inside `assets_dir`. |
| `is_for_sale` | boolean | No | Whether it's buyable. This one is synced. |

### `developer_products[]`

| Field | Type | Required | What it is |
| --- | --- | --- | --- |
| `id` | number | No | Roblox product ID. Same matching rule as game passes. |
| `name` | string | **Yes** | Unique, ignoring capitals. |
| `description` | string | No | Shown on the product. |
| `price` | number | **Yes** | Robux. Required here, unlike game passes. |
| `icon` | string | No | Filename inside `assets_dir`. |
| `is_active` | boolean | No | Read and then ignored. Currently does nothing. |

### `badges[]`

| Field | Type | Required | What it is |
| --- | --- | --- | --- |
| `id` | number | No | Roblox badge ID. Same matching rule as game passes. |
| `name` | string | **Yes** | Unique, ignoring capitals. |
| `description` | string | No | Shown on the badge. |
| `icon` | string | No | Filename inside `assets_dir`. |
| `is_enabled` | boolean | No | Sent as `enabled` when patching. |

Creating a badge costs 100 Robux and needs `badge_payment_source`. If it's missing, rblxsync turns the API error into a readable message.

Roblox cannot list disabled badges. If a disabled badge already exists and your config only names it, rblxsync will not see it and will create a duplicate. Give the entry an `id:` (or run `rblxsync import --badge-id <id>`) to adopt it instead.

### `places[]`

| Field | Type | Required | Default | What it is |
| --- | --- | --- | --- | --- |
| `place_id` | number | **Yes** | | Which place to publish to. |
| `file_path` | string | **Yes** | | Path to a `.rbxl` or `.rbxlx` file. |
| `publish` | boolean | No | `false` | Only `true` places are touched by `rblxsync publish`. |

### A full example

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
    description: "Fly, glow, and skip the queue"
    price: 100
    icon: "vip_pass.png"
    is_for_sale: true

developer_products:
  - name: "Speed Boost"
    description: "Move faster for five minutes"
    price: 50
    icon: "speed_boost.png"

badges:
  - name: "First Win"
    description: "Won your first round"
    icon: "first_win.png"
    is_enabled: true

places:
  - place_id: 1234567890
    file_path: "places/start_place.rbxl"
    publish: true
```

---

## GitHub Action

A composite action. It checks rblxsync out into `.rblxsync-action`, installs stable Rust, caches cargo, builds `--release`, then runs `rblxsync "$COMMAND" --config "$CONFIG" $ARGS`.

### Inputs

| Input | Required | Default | What it is |
| --- | --- | --- | --- |
| `api_key` | **Yes** | | Open Cloud API key. Becomes `ROBLOX_API_KEY`. |
| `command` | No | `run` | `run`, `publish`, `validate`, or `export`. |
| `config` | No | `rblxsync.yml` | Passed through as `--config`. |
| `args` | No | `""` | Extra flags, appended **unquoted**, so they split on spaces. Flags only, nothing containing a space. |
| `roblox_cookie` | No | `""` | `.ROBLOSECURITY` cookie. Becomes `ROBLOX_COOKIE`. Only needed for universe settings. |

### Example

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
        uses: dig1t/rblxsync@v0.2.3
        with:
          api_key: ${{ secrets.ROBLOX_API_KEY }}
          roblox_cookie: ${{ secrets.ROBLOX_COOKIE }}
          command: run
          config: rblxsync.yml
          args: "--dry-run"
```

Anything `run` writes back into `rblxsync.yml` or `rblxsync-lock.yml` on the runner is thrown away when the job finishes. Create new resources locally and commit the IDs, then let CI do the updates.

---

## Environment variables

| Variable | Required | What it is |
| --- | --- | --- |
| `ROBLOX_API_KEY` | Every command except `validate` | Open Cloud API key. Sent as the `x-api-key` header. |
| `ROBLOX_COOKIE` | Only with universe settings | `.ROBLOSECURITY` cookie, for the `develop.roblox.com` configuration PATCH. |
| `RUST_LOG` | No | `env_logger` filter. Defaults to `info`. Set it to `debug` when something looks wrong. |

Both secrets are read from `.env` through `dotenvy`. Keep `.env` in `.gitignore`. Never commit or print either value.

---

## `rblxsync-lock.yml`

Generated state. **Don't hand-edit it**, your changes get overwritten. Commit it so every machine and your CI agree on what already exists. A missing file is treated as empty.

| Key | Type | What it holds |
| --- | --- | --- |
| `universe` | object, optional | Left out when no universe settings are tracked. |
| `game_passes` | map of ID to resource | |
| `developer_products` | map of ID to resource | |
| `badges` | map of ID to resource | |

The universe entry holds `name`, `description`, `genre`, `playable_devices`, `max_players`, and `private_server_cost` (as a string: `disabled`, `0`, or a number).

Each resource entry holds `name`, and optionally `description`, `price`, `is_for_sale` (game passes), `is_enabled` (badges), `icon_hash` (SHA-256 of the local icon, used to skip unchanged uploads), and `icon_asset_id` (the uploaded image asset ID; badges don't store this).

---

## The generated Luau module

Set `output_path` and every successful `run` rewrites a `--!strict` Luau module there. Resources are sorted by ID so the file is stable in git. Strings are escaped for `\`, `"`, `\n`, `\r`, and `\t`.

The type definitions are always emitted in full. A section with nothing in it shows up as an empty table rather than disappearing, so your code can index it without checking.

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
		Name = "Zombie Diner Tycoon",
		Description = "Cook burgers. Survive the night shift.",
		Genre = "adventure",
		PlayableDevices = { "computer", "phone", "tablet" },
		MaxPlayers = 12,
		PrivateServerCost = 100,
	} :: Universe,

	GamePasses = {
		{
			Id = 1122334455,
			Name = "VIP Pass",
			Description = "Skip the queue and get a golden name tag",
			Price = 199,
			IsForSale = true,
		},
		{
			Id = 1122336677,
			Name = "Double Tips",
			Description = "Every customer tips twice as much",
			Price = 149,
			IsForSale = true,
		},
		{
			Id = 1122338899,
			Name = "Golden Spatula",
			Description = "Cook burgers twice as fast",
			Price = 299,
			IsForSale = false,
		},
	} :: { GamePass },

	DeveloperProducts = {
		{
			Id = 2233445566,
			Name = "500 Coins",
			Description = "Starter coin pack",
			Price = 25,
		},
		{
			Id = 2233447788,
			Name = "3000 Coins",
			Description = "Most popular coin pack",
			Price = 100,
		},
		{
			Id = 2233449900,
			Name = "Instant Restock",
			Description = "Refill the fridge without waiting",
			Price = 20,
		},
	} :: { DeveloperProduct },

	Badges = {
		{
			Id = 1234567890123456,
			Name = "First Shift",
			Description = "Survived your first night",
			IsEnabled = true,
		},
		{
			Id = 1234567890987654,
			Name = "Night Owl",
			Description = "Survive ten nights across all your runs",
			IsEnabled = true,
		},
	} :: { Badge },
}
```

That example came from a config with a paid private server, which is why `PrivateServerCost` is the number `100`. Turn private servers off and it becomes the string `"disabled"` instead.

Anything you never set is left out of the table rather than written as `nil`. Icons never appear here at all, they're tracked in the lock file.

`DeveloperProduct` is exactly `{ Id, Name, Description, Price }`. There is no `IsActive` field, so don't write game code expecting one.

### Using it in game

```lua
local Config = require(game.ReplicatedStorage.Shared.Config)

MarketplaceService:PromptGamePassPurchase(player, Config.GamePasses[1].Id)
print(Config.Universe.Name)
```

Don't edit this file by hand. To change its shape, edit `src/output.rs`.

It is not the same as what `export` produces. That one is flat, untyped, and uses snake_case keys.

---

## Permissions and endpoints

rblxsync does not run purely on Open Cloud.

**Universe settings don't use the API key at all.** `develop.roblox.com/v2/.../configuration` is not an Open Cloud endpoint, so those updates go through the `.ROBLOSECURITY` cookie. Some badge operations also use legacy endpoints instead of Open Cloud.

Your `creator` `id` and `type` decide the `creationContext` on uploaded assets (user vs group).

The API key needs these scopes:

| Feature | Scope | Endpoints |
| --- | --- | --- |
| Game passes | read + write | `GET`/`POST`/`PATCH .../game-passes/v1/universes/{uid}/game-passes` |
| Developer products | read + write | `GET`/`POST`/`PATCH .../developer-products/v2/universes/{uid}/developer-products` |
| Badges | read + create/manage | List via `badges.roblox.com/v1/universes/{uid}/badges`; create, update, and icon through the legacy `legacy-badges` / `legacy-publish` endpoints |
| Assets (icons) | upload | `POST /assets/v1/assets` (multipart), then polled at `GET /assets/v1/{operation}` |
| Places | publish | `POST /v1/universes/{uid}/places/{placeId}/versions?versionType=Published` |

A `429` (too many requests) is retried up to 3 times, waiting as long as the `Retry-After` header says.
