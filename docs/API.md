# rblxsync reference

Every command, flag, config field, and file format in one place. If you just want to sync your first game pass, start with the [README](../README.md) instead.

`rblxsync` reads a YAML file and makes your Roblox experience match it: universe settings, game passes, developer products, badges, thumbnails, and places.

---

## CLI commands

```
rblxsync [--config <PATH>] [--env <NAME>] [COMMAND]
```

### Global flags

| Flag | Alias | Default | What it does |
| --- | --- | --- | --- |
| `--config <PATH>` | `-c` | `rblxsync.yml` | Which config file to read. |
| `--env <NAME>` | `-e` | | Which entry of [`environments`](#environments) to use, such as a dev copy of the game. Left out, rblxsync works on the top-level universe. |

Both are global flags, so they work on either side of the subcommand. `rblxsync run --config x.yml` and `rblxsync --config x.yml run` do the same thing.

Type `rblxsync` with no subcommand and you get `run` without `--dry-run`.

### `run`

```
rblxsync run [--dry-run] [--allow-paid-badges]
```

Makes Roblox match your config. Safe to run as many times as you like.

| Flag | What it does |
| --- | --- |
| `--dry-run` | Prints the plan and stops. No writes, no state saved, no `Config.luau` written. |
| `--allow-paid-badges` | Lets badges past the day's free quota be created at 100 Robux each. Without it, a run that needs one stops before changing anything. |

**How each resource is matched**

1. The entry has an `id:` → that ID wins. The name is only a label, so renaming is safe and no duplicate is ever created.
2. No `id:` → matched by name, ignoring capitals, and the match's `id:` is written into your `rblxsync.yml`.
3. No match at all → created, and the new `id:` is written straight back into your `rblxsync.yml`.

With `--env <name>`, the ID in play is the entry's `ids.<name>` instead of `id`, and that's where the write-back goes.

That write-back is a surgical line insert. Your comments and formatting survive it, and it happens the instant the resource is created or matched rather than at the end of the run. If a later step fails, the ID is already saved and the next run adopts it instead of making a second copy. The lock file is saved after every change as well, and a run that stops on an error still writes `Config.luau`, so rerunning picks up where it stopped.

**Places** can't be created through the API, so a place is only synced when it has an ID in the universe being synced (`place_id`, or `ids.<name>` with `--env`); otherwise it's skipped with a warning. A place's `name`, `description`, and `server_size` go out through Open Cloud. rblxsync reads the place first and sends only the fields that differ, because Roblox refuses any place update while the place is open in Team Create. The root place's name, description, and server size are the experience's: if the root place sets one that `universe:` sets too, the run stops before changing anything.

**Icons** are only re-uploaded when the SHA-256 of the local file differs from the hash in the lock file. The icon file goes out in the same request that creates or updates the game pass or developer product.

**Badge cost.** Roblox gives each game 5 free badges per day (GMT), then charges 100 Robux a badge. When a run would create badges, it first reads how many free ones are left today and prints it. Badges past that count need `--allow-paid-badges`, or the run stops before changing anything. Each create sends the cost rblxsync expects (`expectedCost`, 0 while free ones last), and Roblox refuses, without charging, a badge whose real cost is different.

**Thumbnails** are worked out before anything changes. A missing image file stops the run with no changes made. Each upload and delete is written to the lock file as it happens. See [`thumbnails`](#thumbnails).

**Other behavior**

- A new game pass with a `price` is created on sale, unless its `is_for_sale` is `false`.
- Universe settings (`name`, `description`, `playable_devices`, `max_players`, `private_server_cost`) are read from Roblox first, and only the fields that differ are sent. An unchanged private server price is never re-sent, because a price change cancels every private server subscription and Roblox rate-limits it. An environment inherits the top level's settings unless it sets them to `null`.
- A top-level `creator:` is ignored with a warning: icons go up with the pass or product itself, so there's no separate asset to own.
- A `genre:` under `universe:` is ignored with a warning. Roblox only allows a genre change once every 3 months, so set it in the Creator Hub.
- The lock file (`rblxsync-lock.yml`, or `rblxsync-lock.<name>.yml` with `--env <name>`) is saved after every change.
- If `output_path` is set, the Luau module is regenerated after the sync, including one that stopped on an error.

**Paths:** `assets_dir`, `output_path`, and each place's `file_path` are relative to the folder you run rblxsync from. The lock file is always read from and written to the config file's folder.

### `publish`

```
rblxsync publish
```

Uploads and publishes every place in the config with `publish: true`.

- With `--env <name>`, each place goes to its `ids.<name>` place. A place without one is skipped with a warning.
- A `file_path` that doesn't exist is an error.
- One place failing does not stop the others, but the command exits with an error once they're done.

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

**Remote wins.** Where a value exists both locally and on Roblox, the Roblox value overwrites yours. That includes `playable_devices`, `private_server_cost`, and `max_players` when your config sets them; when it doesn't, they stay out, so import never starts managing a setting you left alone. Entries that only exist locally are kept. Lock file entries for resources that no longer exist are dropped.

**Places** come in with their `name`, `description` (when it isn't empty), and `server_size`. The root place gets only `server_size`, or nothing when `universe.max_players` is set, since that's where its size goes: its name, description, and server size are the experience's. A place already in your config keeps its `file_path`, `publish`, and `ids`.

**Environments** are kept as they are, and so is each entry's `ids`. `import` reads the top-level universe only, and refuses `--env`: an environment's names and prices would overwrite the entries every environment shares. To link a dev copy that already has the same items, run `rblxsync run --env dev`. It matches them by name, updates them to match the config, and writes each ID into `ids:`.

Before writing, your existing `rblxsync.yml` is renamed to `rblxsync.old.yml`, then `rblxsync.old1.yml`, `.old2.yml` and so on if that name is taken. Nothing is overwritten without a backup.

Icons and thumbnails are not imported. A `thumbnails` section and its lock entries carry over unchanged.

### `validate`

```
rblxsync validate
```

Checks the config without contacting Roblox. It does **not** need `ROBLOX_API_KEY`.

1. The config file exists.
2. It parses as valid YAML in the expected shape.
3. No two game passes, developer products, or badges share a name (ignoring capitals).
4. Every environment sets its own `universe.id`, different from the top level's and every other environment's, and lists no entries of its own.
5. Every key in an entry's `ids` is an environment.
6. Every place has a `place_id` or `ids`, and a place with `publish: true` has a `file_path`.
7. `thumbnails.game_page` isn't set, and `thumbnails.home_page` has at most 5 images and no `alt_text`.

With `--env <name>`, it also checks that the environment exists and resolves. Exits `1` on failure, logs `Config file is valid.` on success.

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
| `assets_dir` | string | No | `assets` | Folder holding the icon and thumbnail images your config references. |
| `game_passes` | list | No | `[]` | Game passes to sync. |
| `developer_products` | list | No | `[]` | Developer products to sync. |
| `badges` | list | No | `[]` | Badges to sync. |
| `places` | list | No | `[]` | Places to configure and to `publish`. |
| `thumbnails` | object | No | | Home page thumbnails. |
| `environments` | map | No | | Other universes the same entries sync to, by name. See [`environments`](#environments). |
| `badge_payment_source` | string | No | | `"user"` or `"group"`. Needed to create badges. The wallet a badge past the free daily quota is paid from. |
| `output_path` | string | No | | Where `run` writes the typed Luau module, e.g. `src/shared/Config.luau`. |

### `universe`

| Field | Type | Required | What it is |
| --- | --- | --- | --- |
| `id` | number | **Yes** | Universe ID. |
| `name` | string | No | Experience name. Set on the root place. |
| `description` | string | No | Experience description. Set on the root place. |
| `playable_devices` | list of string | No | Any of `computer`, `phone`, `tablet`, `console`, `vr`. Listed devices are turned on, the rest off. An unknown name or an empty list fails validation. |
| `max_players` | number | No | The root place's server size. |
| `private_server_cost` | see below | No | Private server pricing. |

`name`, `description`, and `max_players` go to the root place (the universe's `rootPlace`) as `displayName`, `description`, and `serverSize`. `playable_devices` and `private_server_cost` go to the universe. `run` reads Roblox first and sends only what differs.

A `genre:` here is ignored with a warning from `run` and `validate`.

#### `private_server_cost` values

| You write | Meaning | What Roblox gets |
| --- | --- | --- |
| `"disabled"` (any capitalization) | Private servers off | `privateServerPriceRobux` cleared (`null`) |
| `"free"`, `0`, or `"0"` | Free private servers | `privateServerPriceRobux = 0` |
| `100` or `"100"` | Costs 100 Robux | `privateServerPriceRobux = 100` |

Negative numbers and anything above `u32::MAX` are rejected when the file is parsed.

### `environments`

A map from a name you pick (`dev`, `staging`) to the top-level keys that differ for that copy of the game. `--env <name>` uses the top level with that environment laid over it: a mapping merges key by key, anything else replaces the top-level value, and `null` puts a key back to its default. For a setting like `universe.name` or `thumbnails`, that means rblxsync leaves it alone in that environment.

```yaml
universe:
  id: 123456789
  name: "My Game"

environments:
  dev:
    universe:
      id: 987654321    # required
      name: null       # don't rename the dev copy
    output_path: src/shared/ConfigDev.luau
```

| Rule | Why |
| --- | --- |
| `universe.id` is required, and differs from the top level's and every other environment's | One inherited from the top level would send a dev run to the live game. |
| No `game_passes`, `developer_products`, `badges`, `places`, or `environments` | Entries are shared by every environment. Each keeps its ID for an environment in its own `ids`. |
| The name is letters, digits, `-`, and `_` | It names the lock file, `rblxsync-lock.<name>.yml`. |

One key exists only here: `name_prefix`. It goes in front of the experience's name and every place's name in that environment, so `name_prefix: "[dev] "` turns a place named `Lobby` into `[dev] Lobby` in the dev copy. Pass, product, and badge names stay as they are, because game code looks them up by name.

Universe settings carry over like any other key. If the top level sets `universe.name`, a dev run renames the dev copy to match unless the environment sets its own `name` or `null`.

### `game_passes[]`

| Field | Type | Required | What it is |
| --- | --- | --- | --- |
| `id` | number | No | Roblox game pass ID. Set it and matching uses it instead of the name. Usually written for you on create. |
| `ids` | map of name to number | No | The pass's ID in each environment, like `{ dev: 2222222 }`. `--env` uses it in place of `id`, and writes it for you the same way. |
| `name` | string | **Yes** | Unique, ignoring capitals. Used as the match key when there's no `id`. |
| `description` | string | No | Shown on the pass. |
| `price` | number | No | Robux, at least 1. Left out, the pass is created with no price and isn't for sale. |
| `icon` | string | No | Filename inside `assets_dir`. |
| `is_for_sale` | boolean | No | Whether it's buyable. This one is synced. Left out, a new pass with a `price` is created on sale. |

### `developer_products[]`

| Field | Type | Required | What it is |
| --- | --- | --- | --- |
| `id` | number | No | Roblox product ID. Same matching rule as game passes. |
| `ids` | map of name to number | No | The product's ID in each environment. Same as game passes. |
| `name` | string | **Yes** | Unique, ignoring capitals. |
| `description` | string | No | Shown on the product. |
| `price` | number | **Yes** | Robux, at least 1. Required here, unlike game passes. |
| `icon` | string | No | Filename inside `assets_dir`. |
| `is_active` | boolean | No | Read and then ignored. Currently does nothing. |

### `badges[]`

| Field | Type | Required | What it is |
| --- | --- | --- | --- |
| `id` | number | No | Roblox badge ID. Same matching rule as game passes. |
| `ids` | map of name to number | No | The badge's ID in each environment. Same as game passes. |
| `name` | string | **Yes** | Unique, ignoring capitals. |
| `description` | string | No | Shown on the badge. |
| `icon` | string | No | Filename inside `assets_dir`. |
| `is_enabled` | boolean | No | Sent as `enabled` when patching. |

Creating a badge needs `badge_payment_source`. It's free while the game has free badges left today, and 100 Robux after that, which rblxsync only spends with `--allow-paid-badges`. See the badge cost note under [`run`](#run).

Roblox cannot list disabled badges. If a disabled badge already exists and your config only names it, rblxsync will not see it and will create a duplicate. Give the entry an `id:` (or run `rblxsync import --badge-id <id>`) to adopt it instead.

### `places[]`

| Field | Type | Required | Default | What it is |
| --- | --- | --- | --- | --- |
| `place_id` | number | No | | The place in the top-level universe. A different number from `universe.id`, and you can find it in Studio with `print(game.PlaceId)`. A place needs this or `ids`. |
| `ids` | map of name to number | No | | The same place's ID in each environment. |
| `name` | string | No | | The place's name. |
| `description` | string | No | | The place's description. |
| `server_size` | number | No | | The most players one server of this place holds. |
| `file_path` | string | No | | Path to a `.rbxl` or `.rbxlx` file. Needed with `publish: true`. |
| `publish` | boolean | No | `false` | Only `true` places are touched by `rblxsync publish`. |

`run` syncs `name`, `description`, and `server_size`, and leaves out the ones you don't set. The root place's name, description, and server size are the experience's own, so set each in one spot only: under `universe:` (`name`, `description`, `max_players`) or on the root place here. A run stops if both set the same one.

### `thumbnails`

| Field | Type | Required | What it is |
| --- | --- | --- | --- |
| `home_page` | list | No | The tiles players see on the Roblox home page. Up to 5. Two or more turn on thumbnail personalization, which shows each player the one they're most likely to click. |

`game_page` isn't supported, and a config that sets it fails `validate`. The game page endpoints an API key can use belong to Roblox's localization API: they manage translated thumbnail sets (one per language) and refuse the experience's source language, whose set is the one every player without a translation sees. Upload the game page images in the Creator Hub.

Each entry:

| Field | Type | Required | What it is |
| --- | --- | --- | --- |
| `image` | string | **Yes** | Image file inside `assets_dir`: `.png`, `.jpg`, `.gif`, `.tga`, or `.bmp`. |

How a run treats them:

- An image is uploaded when its file is new or has changed, checked by SHA-256 against the lock file. Moving or renaming a file doesn't upload it again.
- A thumbnail rblxsync uploaded is deleted once you remove its entry or change its file. Deleted one in the Creator Hub? The next run uploads it again.
- Thumbnails rblxsync didn't upload are never deleted.
- `home_page` updates the personalization Roblox is running, which keeps its statistics, or starts one if none is running. Home page thumbnails it stops serving are only deleted if rblxsync uploaded them.
- A missing or empty list leaves that set alone. Removing the whole list deletes nothing.
- An [environment](#environments) gets the same images in its own universe, tracked in its own lock file. Give it its own `thumbnails`, or `thumbnails: null` to leave its thumbnails alone.
- Roblox moderates every image. A rejected image, or a home page one flagged as spam, gets a warning on each run until you replace the file.

Roblox recommends 16:9 images, ideally 1920×1080, and home page images under 3 MB.

### A full example

```yaml
assets_dir: assets/icons
badge_payment_source: "user"
output_path: "src/shared/Config.luau"

universe:
  id: 123456789
  name: "My Awesome Game"
  description: "Updated via rblxsync!"
  playable_devices: ["computer", "phone"]
  max_players: 50
  private_server_cost: "disabled"

environments:
  dev:
    universe:
      id: 987654321
      name: "[dev] My Awesome Game"
    output_path: "src/shared/ConfigDev.luau"

game_passes:
  - name: "VIP Pass"
    id: 1111111
    ids: { dev: 2222222 }
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
    ids: { dev: 1234567891 }
    file_path: "places/start_place.rbxl"
    publish: true
  - place_id: 1234567892
    ids: { dev: 1234567893 }
    name: "Arena"
    server_size: 12

thumbnails:
  home_page:
    - image: "thumbnails/home_a.png"
    - image: "thumbnails/home_b.png"
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
        uses: dig1t/rblxsync@v0.3.0
        with:
          api_key: ${{ secrets.ROBLOX_API_KEY }}
          command: run
          config: rblxsync.yml
          args: "--dry-run"
```

Anything `run` writes back into `rblxsync.yml` or `rblxsync-lock.yml` on the runner is thrown away when the job finishes. Create new resources locally and commit the IDs, then let CI do the updates. Do the same with new or changed thumbnails: thumbnails have no name to match by, so a CI run that can't see them in the lock file uploads them again.

For an environment, pass it in `args`, for example `args: "--env dev"` in a job that runs on your dev branch.

---

## Environment variables

| Variable | Required | What it is |
| --- | --- | --- |
| `ROBLOX_API_KEY` | Every command except `validate` | Open Cloud API key. Sent as the `x-api-key` header. |
| `RUST_LOG` | No | `env_logger` filter. Defaults to `info`. Set it to `debug` when something looks wrong. |

The key is read from `.env` through `dotenvy`. Keep `.env` in `.gitignore`. Never commit or print it.

---

## `rblxsync-lock.yml`

Generated state. **Don't hand-edit it**, your changes get overwritten. Commit it so every machine and your CI agree on what already exists. A missing file is treated as empty.

It sits next to the config file. Each environment has its own, `rblxsync-lock.<name>.yml`, in the same format.

| Key | Type | What it holds |
| --- | --- | --- |
| `universe` | object, optional | Left out when no universe settings are tracked. |
| `game_passes` | map of ID to resource | |
| `developer_products` | map of ID to resource | |
| `badges` | map of ID to resource | |
| `thumbnails` | object, optional | A `home_page` list, one entry per thumbnail rblxsync uploaded. Left out until there is one. |

The universe entry holds `name`, `description`, `playable_devices`, `max_players`, and `private_server_cost` (as a string: `disabled`, `0`, or a number).

Each thumbnail entry holds `image` (the path it was uploaded from), `id` (the homepage thumbnail ID), and `hash` (SHA-256 of the file when it was uploaded).

Each resource entry holds `name`, and optionally `description`, `price`, `is_for_sale` (game passes), `is_enabled` (badges), `icon_hash` (SHA-256 of the local icon, used to skip unchanged uploads), and `icon_asset_id` (the icon asset Roblox made when it created the pass or product; badges don't store this).

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

Universe settings use the API key through Open Cloud. Some badge operations use legacy endpoints instead.

The API key needs these scopes:

| Feature | Scope | Endpoints |
| --- | --- | --- |
| Universe settings | `universe` write (`universe:write`) | Devices and private servers: `PATCH /cloud/v2/universes/{uid}?updateMask=...`, after a `GET` of the universe |
| Game passes | read + write | `GET`/`POST`/`PATCH .../game-passes/v1/universes/{uid}/game-passes` |
| Developer products | read + write | `GET`/`POST`/`PATCH .../developer-products/v2/universes/{uid}/developer-products` |
| Badges | read + create/manage | List via `badges.roblox.com/v1/universes/{uid}/badges` and the free quota via `.../free-badges-quota` (no auth); create, update, and icon through the legacy `legacy-badges` / `legacy-publish` endpoints |
| Places | read + write (`universe.place:write`) | Settings, and the experience's name, description, and server size on the root place: `GET` then `PATCH /cloud/v2/universes/{uid}/places/{placeId}?updateMask=...`. Publish (`universe-places:write`): `POST /universes/v1/{uid}/places/{placeId}/versions?versionType=Published`, the file as `application/octet-stream` (`.rbxl`) or `application/xml` (`.rbxlx`) |
| Game page thumbnails | none | Not synced. The `legacy-game-internationalization/v1/game-thumbnails/...` endpoints (`legacy-universe:manage`) only manage translated sets and refuse the source language (error 26), and no API-key endpoint reaches the source-language set. A config with `thumbnails.game_page` fails `validate`. |
| Home page thumbnails | `universe.thumbnail:read` + `universe.thumbnail:write` | `thumbnail-personalization-api/v1/universes/{uid}/thumbnails` to list, upload (then polled at `.../thumbnails/uploads/status`), and delete; `.../personalization` to read, create, and update. Roblox marks this API experimental |

A `429` (too many requests) is retried up to 3 times, waiting as long as the `Retry-After` header says.
