# rblxsync — CLI, Environment, CI, Permissions

## Installation

The binary is `rblxsync`.

```bash
# From source (works today)
cargo install --path .

# Tool managers — ONLY once binary releases exist. Pin to a published tag.
#   rokit.toml / aftman.toml
#   [tools]
#   rblxsync = "dig1t/rblxsync@0.1.0"
```

> As of v0.1.0 there is no published binary-release pipeline — the GitHub Action
> builds from source, and the source tag `v0.1.0` exists. Prefer **from source**
> until pre-built binaries are published. Do not assume `rokit add` works yet.

## CLI

```
rblxsync [--config <PATH>] [COMMAND]
```

The `--config` / `-c` flag is **global** and must come **before** the subcommand:

```bash
rblxsync --config production.yml run    # ✅ correct
rblxsync run --config production.yml    # ✗ clap rejects this
```

If no subcommand is given, it defaults to `run` (not dry-run).

### `run [--dry-run]`

Syncs universe settings + game passes, developer products, badges. Idempotent
(match by name, create/PATCH). Icons re-upload only when their local SHA-256
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

Publishes every place with `publish: true` (always a **Published** version — no
"Saved" option). Does NOT need the cookie. Per-place errors are logged but not
fatal; a place with a missing `file_path` is skipped and the rest continue.
Publishing makes the place live — confirm with the user.

### `validate`

Parses and checks the config. No API key, no network. Rejects duplicate
case-insensitive names. Exits `1` on failure.

### `export [-o/--output PATH] [--lua]`

Pulls live game passes, products, badges and dumps a **flat** Luau/Lua table.
Default filename `config.luau` (`config.lua` with `--lua` — content is identical;
`--lua` only changes the default name). This is a **one-way snapshot** for
inspection/migration. It is NOT a valid `rblxsync.yml`, and NOT the richer shape
that `run` writes to `output_path`. Flat shape:

```lua
return {
    game_passes = { { name = "VIP Pass", id = 123456, price = 100 } },
    developer_products = { { name = "Speed Boost", id = 234567, price = 50 } },
    badges = { { name = "First Win", id = 345678 } },
}
```

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

Anyone with this cookie can access the account — treat it like a password, store
it only in `.env` or a CI secret, never in the repo or chat. Don't fetch or echo
it yourself; instruct the user to place it.

## Open Cloud API key permissions

Universe **settings** updates do **not** use the API key — they use cookie auth
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

Composite action: checks out rblxsync into `.rblxsync-action`, sets up stable
Rust, caches cargo, builds `--release`, then runs
`rblxsync "$COMMAND" --config "$CONFIG" $ARGS`.

### Inputs

| Input | Required | Default | Notes |
| --- | --- | --- | --- |
| `api_key` | **Yes** | – | Open Cloud key → `ROBLOX_API_KEY`. |
| `command` | No | `run` | `run` / `publish` / `validate` / `export`. |
| `config` | No | `rblxsync.yml` | Passed as `--config`. |
| `args` | No | `""` | Extra flags, appended **unquoted**. |
| `roblox_cookie` | No | `""` | `.ROBLOSECURITY` → `ROBLOX_COOKIE`. Only for universe settings. |

> **`args` word-splits and cannot be quoted.** Multiple flags like
> `--dry-run --foo` split correctly, but **any single argument containing a space
> is broken apart** — there is no quoting mechanism. Use only space-free flags.

> **Pin to a published ref.** A `v0.1.0` tag exists; a moving `@v1` major tag is
> **not** published. Do not reference `@v1` until it exists. `@main` works but is
> unpinned.

Store `ROBLOX_API_KEY` (and `ROBLOX_COOKIE` if needed) under
**Settings → Secrets and variables → Actions**. See
`assets/github-workflow-sync.yml` for a copy-paste workflow.
