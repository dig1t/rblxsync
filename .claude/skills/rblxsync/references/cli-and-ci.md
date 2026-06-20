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
(match by `id` when an entry has one, else by name; create/PATCH). Icons re-upload
only when their local SHA-256 differs from the lock file.

- **Preflight gate:** before any mutation (in both real and `--dry-run` mode)
  `run` validates everything knowable up front and **aborts listing every
  problem if any** — so a failed run never half-applies (no resources created,
  no Robux spent). It checks: referenced icon files exist; passes/products with
  an icon have a `creator:`; a badge that would be **created** has an icon
  (Roblox requires one) and a `badge_payment_source`.
- `--dry-run`: previews changes, makes **no** mutating HTTP calls, does **not**
  write state, does **not** write `Config.luau`. Runs the preflight too, so a
  preview surfaces the errors above. Always run this first.
- Requires `ROBLOX_COOKIE` if any `universe.*` setting is present (see below).
- On success writes `rblxsync-lock.yml`, and regenerates `Config.luau` if
  `output_path` is set.
- When it **creates** a resource whose entry had no `id`, it writes the new `id`
  back into that `rblxsync.yml` entry (a surgical edit that preserves comments)
  so future renames are safe. If it can't locate the entry, it warns to run
  `import` to backfill ids.

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

### `import [--universe-id ID] [--place-id ID]…`

Pulls a live experience's metadata **down** into `rblxsync.yml` **and**
`rblxsync-lock.yml` — the opposite direction of `run`. Use it to adopt rblxsync on
an existing game, or to absorb a resource rblxsync doesn't know about yet. Unlike
`export`, the output **is** a valid config you keep using.

- **Universe id:** `--universe-id <id>`, else `universe.id` from an existing
  config; errors if neither is available.
- **Reconciliation (destructive on conflicts):** remote is authoritative and
  overwrites matching local entries (matched by `id`, else case-insensitive name);
  entries that exist locally but not on remote are **kept**; lockfile entries that
  are neither in the yml nor on remote are **dropped** as stale.
- **Stable ids:** every imported game pass / developer product / badge is written
  with its Roblox `id`, so they're immediately rename-safe.
- **Backup:** an existing `rblxsync.yml` is renamed to `rblxsync.old.yml` (then
  `rblxsync.old1.yml`, `.old2.yml`, …) before the new one is written. Comments in
  the old file are not carried over — the backup is the record.
- **Icons:** not imported (the asset can't be downloaded to a local file). Add an
  `icon:` path later and the next `run` uploads it.
- **Places:** the API key can only auto-discover the **root** place (added with
  `file_path: ""`, `publish: false`). Pass `--place-id <id>` (repeatable) for
  additional places; each is fetched via the universe-scoped place endpoint, which
  also **verifies the place belongs to this experience** — an id from a different
  experience (or a typo) is warned and skipped, not imported. `.rbxl` files still
  can't be downloaded, so set each `file_path` manually before publishing.
- Needs only `ROBLOX_API_KEY` (no cookie).

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
| Universe / Places (import) | read | `GET /cloud/v2/universes/{uid}` and `GET /cloud/v2/universes/{uid}/places/{placeId}` |

`429` responses are retried up to 3 times honoring `Retry-After`.

## GitHub Action

Composite action: checks out rblxsync into `.rblxsync-action`, sets up stable
Rust, caches cargo, builds `--release`, then runs
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
> is broken apart** — there is no quoting mechanism. Use only space-free flags.

> **Pin to a published ref.** A `v0.1.0` tag exists; a moving `@v1` major tag is
> **not** published. Do not reference `@v1` until it exists. `@main` works but is
> unpinned.

Store `ROBLOX_API_KEY` (and `ROBLOX_COOKIE` if needed) under
**Settings → Secrets and variables → Actions**. See
`assets/github-workflow-sync.yml` for a copy-paste workflow.
