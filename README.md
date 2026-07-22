# rblxsync

`rblxsync` is a Rust-based CLI tool and GitHub Action for declaratively managing Roblox experience metadata via the Open Cloud API. Define your Universe settings, Game Passes, Developer Products, Badges, and Places in a single YAML file (`rblxsync.yml`) and sync them to Roblox with one command.

Documentation: https://dig1t.github.io/rbxsync/

> Full reference documentation lives in [docs/API.md](docs/API.md): CLI commands, the configuration schema, GitHub Action inputs, environment variables, the lock file, generated Luau output, and Open Cloud permissions.

## Features

- **Declarative configuration**: manage all game metadata in `rblxsync.yml`.
- **Idempotent sync**: resources are matched by name (case-insensitive); created if missing, updated if present.
- **Icon management**: icons for Game Passes, Products, and Badges are re-uploaded only when the local file changes (SHA-256 checksum).
- **Place publishing**: publish `.rbxl` files to specific Place IDs.
- **Export**: dump existing Roblox resources to a flat Luau/Lua table (a one-way snapshot, not a config you can feed back in).
- **Auto-generated config**: write a typed Luau module (`output_path`) containing all resource IDs after each sync.
- **CI/CD ready**: ships as a GitHub Action.

## Installation

> **Note:** The tool-manager and release-binary installs below assume published release artifacts. At the time of writing the repository publishes the GitHub Action (which builds from source) and the `v0.2.2` source tag; there is no binary-release pipeline yet. Until pre-built binaries are published, prefer **From Source**.

### From Source

```bash
cargo install --path .
```

### Rokit / Aftman / Foreman

Once binary releases are published, you can install via a tool manager. Pin to a published tag (currently `0.2.2`):

```toml
# rokit.toml or aftman.toml
[tools]
rblxsync = "dig1t/rblxsync@0.2.2"
```

```bash
rokit add dig1t/rblxsync@0.2.2   # Rokit
aftman install                   # Aftman
foreman install                  # Foreman
```

### GitHub Releases

Pre-built binaries, if available, are published on the [Releases](https://github.com/dig1t/rblxsync/releases) page.

## Quick Start

1. Set your Open Cloud API key. Create a `.env` file in your project root:

   ```bash
   ROBLOX_API_KEY=your_api_key_here
   ```

   > **Security:** `.env` is gitignored and must never be committed. The API key is a secret.

2. Create a minimal `rblxsync.yml` (only `universe.id` is required):

   ```yaml
   universe:
     id: 123456789

   game_passes:
     - name: "VIP Pass"
       price: 100
   ```

3. Sync:

   ```bash
   rblxsync run
   ```

That's it. Add Game Passes, Developer Products, Badges, Places, and universe settings as needed. See [docs/API.md](docs/API.md#configuration-schema) for the full schema.

## CLI Commands

The `-c` / `--config` flag is **global** and must come **before** the subcommand (clap parses it on the top-level command, not the subcommand):

```bash
rblxsync --config production.yml run    # correct
rblxsync run --config production.yml    # WRONG - clap will reject this
```

| Command | Description |
|---------|-------------|
| `rblxsync run` | Sync universe settings + assets (Game Passes, Products, Badges). This is the default if no subcommand is given. Add `--dry-run` to preview. |
| `rblxsync publish` | Publish `.rbxl` files defined in the `places` section. Always creates a **Published** version (there is no "Saved" option). |
| `rblxsync export` | Fetch existing resources from Roblox and dump them to a flat Luau/Lua table. See the note below. |
| `rblxsync validate` | Validate `rblxsync.yml` without contacting the API. |

```bash
rblxsync run --dry-run                 # preview changes
rblxsync export --output Config.luau   # default extension
rblxsync export --output Config.lua --lua
```

**About `export`:** it produces a flat, **untyped** table with snake_case keys (`game_passes`, `developer_products`, `badges`; `game_passes` and `developer_products` hold `name`/`id`/`price`, while `badges` hold only `name`/`id`). The `--lua` flag only changes the default output filename; the generated content is byte-identical. This output is a **one-way snapshot for inspection/migration**; it is *not* the typed module produced by `output_path`, and it is *not* a valid `rblxsync.yml` you can pass back into `rblxsync run`.

Full details: [docs/API.md#cli-commands](docs/API.md#cli-commands).

## Configuration Overview

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

See the complete, field-by-field schema in [docs/API.md#configuration-schema](docs/API.md#configuration-schema). A working sample lives in [`rblxsync.example.yml`](rblxsync.example.yml).

## GitHub Action

The action checks out and builds `rblxsync` from source (`cargo build --release`), then runs the requested command.

```yaml
name: Sync Roblox Experience

on:
  push:
    branches: [main]

jobs:
  sync:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Sync Roblox metadata
        uses: dig1t/rblxsync@v0.2.2
        with:
          api_key: ${{ secrets.ROBLOX_API_KEY }}
          command: run
```

> Pin the action to a published ref. A `v0.2.2` tag exists; a `@v1` moving major tag is **not** published, so do not reference `@v1` until one is created.

### Action Inputs

| Input | Required | Default | Description |
|-------|----------|---------|-------------|
| `api_key` | **Yes** | – | Roblox Open Cloud API key. |
| `command` | No | `run` | One of `run`, `publish`, `validate`, `export`. |
| `config` | No | `rblxsync.yml` | Path to the config file (passed as the global `--config`). |
| `args` | No | `""` | Extra flags appended to the command, e.g. `--dry-run`. |
| `roblox_cookie` | No | `""` | `.ROBLOSECURITY` cookie (see Environment Variables). |

> **`args` is word-split, not shell-quoted.** The action passes `args` unquoted so multiple flags (e.g. `--dry-run --foo`) split into separate arguments. As a result, **any single argument containing spaces will be broken apart**; there is no quoting mechanism. Use only flag-style arguments without embedded spaces.

```yaml
- name: Preview changes
  uses: dig1t/rblxsync@v0.2.2
  with:
    api_key: ${{ secrets.ROBLOX_API_KEY }}
    command: run
    args: --dry-run

- name: Sync with universe settings (requires cookie)
  uses: dig1t/rblxsync@v0.2.2
  with:
    api_key: ${{ secrets.ROBLOX_API_KEY }}
    roblox_cookie: ${{ secrets.ROBLOX_COOKIE }}
    command: run
```

Store `ROBLOX_API_KEY` (and optionally `ROBLOX_COOKIE`) as repository secrets under **Settings → Secrets and variables → Actions**.

Full input/output reference: [docs/API.md#github-action](docs/API.md#github-action).

## Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `ROBLOX_API_KEY` | **Yes** | Open Cloud API key. |
| `ROBLOX_COOKIE` | Conditional | `.ROBLOSECURITY` cookie. Required when universe settings are present (see gotcha below). |

Set them in a gitignored `.env` file:

```bash
ROBLOX_API_KEY=your_api_key_here
ROBLOX_COOKIE=your_roblosecurity_cookie_here
```

> **Security:** `.env` is gitignored. Never commit it, never print the API key or cookie. Anyone with your `.ROBLOSECURITY` cookie can access your account.

**Cookie gotcha:** `rblxsync run` requires `ROBLOX_COOKIE` whenever *any* universe field is set, including `genre` and `max_players`, which are local-only and never trigger a cookie API call. So a config that sets only `genre` will still hard-fail demanding `ROBLOX_COOKIE` even though no cookie request would be made. If you only want local tracking of those fields, be aware of this behavior.

### A note on permissions and endpoints

`rblxsync` does **not** rely solely on Open Cloud API-key scopes. Universe *settings* updates use **cookie authentication** against `develop.roblox.com`; an API key alone cannot update universe settings. Several badge operations also use legacy endpoints (`badges.roblox.com`, `apis.roblox.com/legacy-badges`, `legacy-publish`) rather than pure Open Cloud. The required API-key scopes (Game Passes, Developer Products, Assets, Places) plus the cookie requirement and a per-operation endpoint map are documented in [docs/API.md#open-cloud-permissions](docs/API.md#open-cloud-permissions).

## Lock File & Generated Config

### `rblxsync-lock.yml`

`rblxsync` maintains a `rblxsync-lock.yml` file tracking resource IDs (Game Pass / Product / Badge IDs), icon file hashes, and universe settings state. **Commit it** to version control for idempotent syncs across environments.

> This file is **generated**. Do not hand-edit it; edits are overwritten on the next sync.

### `output_path`: generated Luau module

If you set `output_path`, a typed Luau module is regenerated every time `rblxsync run` completes. It always emits the full type definitions for `Universe`, `GamePass`, `DeveloperProduct`, and `Badge`, plus the corresponding tables (sections appear as empty tables when you have no resources of that kind; they are never omitted). Keys are PascalCase.

The generated `DeveloperProduct` type is exactly `{ Id, Name, Description, Price }`. There is **no** `IsActive` field, so do not type your game code against one. Example of the generated shape:

```luau
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
		MaxPlayers = 50,
	} :: Universe,

	GamePasses = {
		{ Id = 111111111, Name = "VIP Pass", Price = 100, IsForSale = true },
	} :: { GamePass },

	DeveloperProducts = {} :: { DeveloperProduct },
	Badges = {} :: { Badge },
}
```

> Do not edit `Config.luau` by hand; it is regenerated on every `run`. Edit `src/output.rs` to change the format.

Full reference: [docs/API.md#generated-luau-output-configluau](docs/API.md#generated-luau-output-configluau).

## Contributing

```bash
cargo build                                      # build
cargo test                                       # test
cargo clippy --all-targets -- -D warnings        # lint
cargo fmt                                         # format
cargo run -- validate                            # validate a config
```

Run the tests and clippy before opening a PR.

## License

MIT
