# rblxsync: CLI, Environment, CI, Permissions

## Installation

The binary is `rblxsync`.

```bash
# Recommended: tool-manager pin (Rokit / Aftman)
rokit add dig1t/rblxsync@0.2.3

#   rokit.toml / aftman.toml
#   [tools]
#   rblxsync = "dig1t/rblxsync@0.2.3"

# Second choice: download a zip from the GitHub Releases page.
# Fallback: build from source
cargo install --path .
```

> Every `VERSION` bump publishes a release with four zips:
> `rblxsync-v<version>-x86_64-unknown-linux-gnu.zip`, `-x86_64-apple-darwin.zip`
> (Intel Mac), `-aarch64-apple-darwin.zip` (Apple Silicon), and
> `-x86_64-pc-windows-msvc.zip`. Each zip holds a bare `rblxsync` executable
> (`rblxsync.exe` on Windows), named so Aftman/Rokit resolve it. Verify the
> install with `rblxsync --version` (prints `rblxsync 0.2.3`).

## CLI

```
rblxsync [--config <PATH>] [COMMAND]
```

The `--config` / `-c` flag is **global** (clap `global = true`) and works on
either side of the subcommand:

```bash
rblxsync --config production.yml run    # works
rblxsync run --config production.yml    # identical
```

If no subcommand is given, it defaults to `run` (not dry-run).

### `run [--dry-run] [--allow-paid-badges]`

Syncs universe settings + game passes, developer products, badges, thumbnails. Idempotent:
an entry with an `id:` is matched by that id; without one, by name
(case-insensitive); no match creates the resource and writes the new `id:` back
into `rblxsync.yml` immediately. Icons re-upload only when their local SHA-256
differs from the lock file; a new pass or product is created with its icon, and
a new pass with a `price` is created on sale unless `is_for_sale: false`.
Thumbnails upload only when an image is new or changed, and a changed or removed
one has its old upload deleted. Thumbnails rblxsync didn't upload are left
alone, and `home_page` updates the running personalization in place.

- `--dry-run`: previews changes, makes **no** mutating HTTP calls, does **not**
  write state, does **not** write `Config.luau`. Always run this first.
- `--allow-paid-badges`: lets badges past the day's free quota (5 per game per
  day, GMT) be created at 100 Robux each. Without it, `run` reads and prints the
  quota and stops before changing anything if it would need a paid badge. Only
  pass it with the user's explicit go-ahead.
- Syncs universe settings: GETs the universe and root place first, sends only
  the fields that differ (`name` / `description` / `max_players` to the root
  place, scope `universe.place:write`; `playable_devices` / `private_server_cost`
  to the universe, scope `universe:write`). An unchanged private server price is
  never re-sent. A `genre:` is ignored with a warning.
- Syncs place `name` / `description` / `server_size` (GET, then PATCH only the
  changed fields; scope `universe.place:write`).
- On success writes `rblxsync-lock.yml` (`rblxsync-lock.<env>.yml` with
  `--env`), and regenerates `Config.luau` if `output_path` is set.
- A name match also writes its `id:` (or `ids.<env>`) into `rblxsync.yml`.

> **Path note:** the lock file is read from and written to the config file's
> folder. `assets_dir`, `output_path`, and place `file_path`s are relative to
> the working directory, so run rblxsync from the project root.

### `--env <name>` (global)

Uses `environments.<name>` from the config: that universe, its overrides, each
entry's `ids.<name>` as its id, and its own lock file. Works with `run`,
`publish`, `export`, and `validate`. `import` refuses it, since an environment's
values would overwrite the shared entries; to adopt a dev copy that already has
the items, `run --env <name> --dry-run`, then `run --env <name>` (matches by
name, PATCHes to the config, writes `ids.<name>`). Place ids go in by hand.

### `publish`

Publishes every place with `publish: true` (always a **Published** version, no
"Saved" option). Per-place errors are logged but not
fatal; a place with a missing `file_path` is skipped and the rest continue.
Publishing makes the place live; confirm with the user.

### `validate`

Parses and checks the config. No API key, no network. Rejects duplicate
case-insensitive names. Exits `1` on failure.

### `export [-o/--output PATH] [--lua]`

Pulls live game passes, products, badges and dumps a **flat** Luau/Lua table.
Default filename `config.luau` (`config.lua` with `--lua`; content is identical,
`--lua` only changes the default name). This is a **one-way snapshot** for
reading only. It is NOT a valid `rblxsync.yml`, and NOT the richer shape
that `run` writes to `output_path`. To turn an existing game into a config, use
`import` instead. Flat shape:

```lua
return {
    game_passes = { { name = "VIP Pass", id = 123456, price = 100 } },
    developer_products = { { name = "Speed Boost", id = 234567, price = 50 } },
    badges = { { name = "First Win", id = 345678 } },
}
```

### `import [--universe-id <id>] [--place-id <id>]... [--badge-id <id>]...`

Pulls an existing experience down into **both** `rblxsync.yml` and
`rblxsync-lock.yml`. This is how you adopt rblxsync on a game that already has
live game passes, products, and badges, or absorb a resource rblxsync doesn't
know about yet.

- `--universe-id` falls back to `universe.id` in an existing config if omitted.
- `--place-id` is repeatable. The API key only auto-discovers the root place;
  each extra place needs one flag.
- `--badge-id` is repeatable. Roblox's badge listing omits **disabled** badges,
  so a disabled badge can only be imported by id. Without it, a config naming a
  disabled badge will create a duplicate.
- Remote is authoritative on conflicts: remote values overwrite matching local
  ones, local-only entries are kept, stale lock entries are dropped.
- Places get `name`, `description` (if not empty), and `server_size`; the root
  place gets only `server_size` (its name/description go to `universe:`).
  `environments` and each entry's `ids` are kept.
- An existing `rblxsync.yml` is backed up to `rblxsync.old.yml` first (then
  `rblxsync.old1.yml`, `.old2.yml`, ...).
- Icons and thumbnails are not imported. An existing `thumbnails` section and
  its lock entries carry over unchanged.

## Environment variables

| Variable | Required | Notes |
| --- | --- | --- |
| `ROBLOX_API_KEY` | Yes (all but `validate`) | Open Cloud API key, sent as `x-api-key`. |
| `RUST_LOG` | No | `env_logger` filter; defaults to `info`. Use `RUST_LOG=debug` to troubleshoot. |

The key loads from a gitignored `.env` via `dotenvy`. **Never commit or print
it.** Add to `.env`:

```bash
ROBLOX_API_KEY=your_api_key_here
```

## Open Cloud API key permissions

The API key needs these scopes. Some badge operations use legacy endpoints
instead of Open Cloud.


| Feature | Scope | Endpoint(s) |
| --- | --- | --- |
| Universe settings | `universe` write (`universe:write`) | Devices and private servers: `GET` then `PATCH /cloud/v2/universes/{uid}?updateMask=...` |
| Game Passes | read + write | `game-passes/v1/universes/{uid}/game-passes` |
| Developer Products | read + write | `developer-products/v2/universes/{uid}/developer-products` |
| Badges | read + create/manage | list and free quota via `badges.roblox.com`; create/update/icon via legacy `legacy-badges` / `legacy-publish` |
| Places | read + write (`universe.place:write`) | Settings, and the experience's name / description / server size on the root place: `GET`/`PATCH /cloud/v2/universes/{uid}/places/{placeId}?updateMask=...`; publish (`universe-places:write`): `POST /universes/v1/{uid}/places/{placeId}/versions?versionType=Published` |
| Game page thumbnails | none | Not synced: the localization endpoints only manage translated sets and refuse the source language (error 26). `thumbnails.game_page` fails `validate`. |
| Home page thumbnails | `universe.thumbnail:read` + `universe.thumbnail:write` | `thumbnail-personalization-api/v1/universes/{uid}/thumbnails` and `.../personalization` (experimental) |

`429` responses are retried up to 3 times honoring `Retry-After`.

## GitHub Action

Composite action: checks out rblxsync at the ref you pinned
(`ref: ${{ github.action_ref }}`, so `uses: dig1t/rblxsync@v0.2.3` builds
exactly that tag) into `.rblxsync-action`, sets up stable Rust, caches cargo,
builds `--release`, then runs
`rblxsync "$COMMAND" --config "$CONFIG" $ARGS`.

### Inputs

| Input | Required | Default | Notes |
| --- | --- | --- | --- |
| `api_key` | **Yes** | – | Open Cloud key → `ROBLOX_API_KEY`. |
| `command` | No | `run` | `run` / `publish` / `validate` / `export` / `import`. |
| `config` | No | `rblxsync.yml` | Passed as `--config`. |
| `args` | No | `""` | Extra flags, appended **unquoted**. |

> **`args` word-splits and cannot be quoted.** Multiple flags like
> `--dry-run --foo` split correctly, but **any single argument containing a space
> is broken apart**; there is no quoting mechanism. Use only space-free flags.

> **Pin to a published ref.** Published tags: `v0.1.0`, `v0.1.1`, `v0.2.2`, `v0.2.3`
> (latest). A moving `@v1` major tag is **not** published. Do not reference
> `@v1` until it exists. `@main` works but is unpinned.

> **Write-backs stay on the runner.** New ids and thumbnail uploads land in
> `rblxsync.yml` / `rblxsync-lock.yml` on the runner and vanish when the job
> ends. Create new resources and sync new or changed thumbnails locally, commit
> both files, then let CI do the updates. Thumbnails have no name to match by,
> so a CI-only setup uploads them again on every run.

Store `ROBLOX_API_KEY` under
**Settings → Secrets and variables → Actions**. See
`assets/github-workflow-sync.yml` for a copy-paste workflow.
