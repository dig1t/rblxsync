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

### `run [--dry-run]`

Syncs universe settings + game passes, developer products, badges. Idempotent:
an entry with an `id:` is matched by that id; without one, by name
(case-insensitive); no match creates the resource and writes the new `id:` back
into `rblxsync.yml` immediately. Icons re-upload only when their local SHA-256
differs from the lock file.

- `--dry-run`: previews changes, makes **no** mutating HTTP calls, does **not**
  write state, does **not** write `Config.luau`. Always run this first.
- Requires `ROBLOX_COOKIE` if any `universe.*` setting is present (see below).
- On success writes `rblxsync-lock.yml`, and regenerates `Config.luau` if
  `output_path` is set.

> **Path note:** the lock file is *loaded* from the config file's parent dir but
> *saved* to the current working directory. Run rblxsync from the directory that
> holds `rblxsync-lock.yml` so state stays consistent.

### `publish`

Publishes every place with `publish: true` (always a **Published** version, no
"Saved" option). Does NOT need the cookie. Per-place errors are logged but not
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
- An existing `rblxsync.yml` is backed up to `rblxsync.old.yml` first (then
  `rblxsync.old1.yml`, `.old2.yml`, ...).
- Icons are not imported.

## Environment variables

| Variable | Required | Notes |
| --- | --- | --- |
| `ROBLOX_API_KEY` | Yes (all but `validate`) | Open Cloud API key, sent as `x-api-key`. |
| `ROBLOX_COOKIE` | Conditional | `.ROBLOSECURITY` cookie. Required only when universe settings are defined. |
| `RUST_LOG` | No | `env_logger` filter; defaults to `info`. Use `RUST_LOG=debug` to troubleshoot. |

Both secrets load from a gitignored `.env` via `dotenvy`. **Never commit or print
them.** Add to `.env`:

```bash
ROBLOX_API_KEY=your_api_key_here
ROBLOX_COOKIE=your_roblosecurity_cookie_here   # only if syncing universe settings
```

### Getting the `.ROBLOSECURITY` cookie (only if needed)

1. Log into roblox.com in a browser.
2. DevTools (F12) → Application → Cookies → copy the value of `.ROBLOSECURITY`.

Anyone with this cookie can access the account. Treat it like a password, store
it only in `.env` or a CI secret, never in the repo or chat. Don't fetch or echo
it yourself; instruct the user to place it.

## Open Cloud API key permissions

Universe **settings** updates do **not** use the API key; they use cookie auth
against `develop.roblox.com`. The API key needs these scopes:

| Feature | Scope | Endpoint(s) |
| --- | --- | --- |
| Game Passes | read + write | `game-passes/v1/universes/{uid}/game-passes` |
| Developer Products | read + write | `developer-products/v2/universes/{uid}/developer-products` |
| Badges | read + create/manage | list via `badges.roblox.com`; create/update/icon via legacy `legacy-badges` / `legacy-publish` |
| Assets (icons) | upload | `POST /assets/v1/assets` (multipart), polled at `GET /assets/v1/{operation}` |
| Places | publish | `POST /v1/universes/{uid}/places/{placeId}/versions?versionType=Published` |

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
| `roblox_cookie` | No | `""` | `.ROBLOSECURITY` → `ROBLOX_COOKIE`. Only for universe settings. |

> **`args` word-splits and cannot be quoted.** Multiple flags like
> `--dry-run --foo` split correctly, but **any single argument containing a space
> is broken apart**; there is no quoting mechanism. Use only space-free flags.

> **Pin to a published ref.** Published tags: `v0.1.0`, `v0.1.1`, `v0.2.2`, `v0.2.3`
> (latest). A moving `@v1` major tag is **not** published. Do not reference
> `@v1` until it exists. `@main` works but is unpinned.

Store `ROBLOX_API_KEY` (and `ROBLOX_COOKIE` if needed) under
**Settings → Secrets and variables → Actions**. See
`assets/github-workflow-sync.yml` for a copy-paste workflow.
