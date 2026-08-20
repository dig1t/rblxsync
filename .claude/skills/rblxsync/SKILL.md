---
name: rblxsync
description: >-
  Declaratively manage Roblox experience metadata (Universe settings, Game
  Passes, Developer Products, Badges, Places) from a YAML file via the rblxsync
  CLI / GitHub Action. Use when setting up rblxsync in a Roblox project, writing
  or editing rblxsync.yml, syncing/publishing game passes, developer products,
  badges, places or universe settings, wiring the generated Config.luau into
  Luau game code, debugging ROBLOX_API_KEY / ROBLOX_COOKIE errors, or adding
  rblxsync to CI, or when adopting rblxsync on a game that already has live
  monetization. Triggers on "rblxsync", "rblxsync.yml", "sync game passes",
  "Open Cloud metadata", "publish place", "Config.luau", "rblxsync-lock.yml",
  "rblxsync import".
---

# rblxsync

`rblxsync` is a Rust CLI + GitHub Action that **declaratively** manages Roblox
experience metadata via the Open Cloud API. One YAML file (`rblxsync.yml`) is the
source of truth for Universe settings, Game Passes, Developer Products, Badges,
and Places. Running it is idempotent: resources are matched by **id** when the
config entry has one, otherwise by **name** (case-insensitive), created if
missing, updated (PATCH) if present.

Use this skill to set rblxsync up in a user's Roblox project, author/edit their
config, run syncs safely, and wire the generated `Config.luau` into game code.

## Mental model (read this first)

- **`rblxsync.yml`** = desired state, hand-edited, committed.
- **`rblxsync-lock.yml`** = generated state (resource IDs + icon hashes).
  **Commit it.** Never hand-edit it: it is overwritten on the next sync.
- **`Config.luau`** (only if `output_path` is set) = generated, typed Luau module
  of all resource IDs. Game code `require`s it. Never hand-edit it.
- **Matching is id-first.** Each `game_passes[]`, `developer_products[]`, and
  `badges[]` entry takes an optional `id:`. Resolution order on `run`:
  1. Entry has `id:` -> matched by that id. The `name` is only a label.
  2. No `id:` -> matched by name, case-insensitive.
  3. No match -> created, and the new `id:` is written back into the user's
     `rblxsync.yml` right away (a line insert that keeps their comments and
     formatting), so a later failure can't lose it and the next run adopts the
     resource instead of duplicating it.
- **Renaming is safe once an entry has its `id:`**, which is the normal state
  after the first successful run. Change the `name:` and rblxsync PATCHes the
  existing resource.
- **An entry with no `id:` is the one to watch.** Renaming that one means no
  match by the new name, so rblxsync creates a second resource and leaves the
  original selling. Duplicate badges also cost 100 Robux each.
  - Fix: give the entry its id rather than warning the user off the rename.
    Run `rblxsync import` (or `import --badge-id <id>` for a disabled badge) to
    pull the real id into the config, then rename freely.
  - Do not hand-edit `rblxsync-lock.yml` to preserve an id. That was the old
    workaround and it is no longer the answer.
  - `--dry-run` still proves it: an unexpected "CREATE" for something that
    already exists means the entry is missing its id.

## Golden rules

1. **Never run a mutating sync without previewing first.** Always do
   `rblxsync run --dry-run` and show the user the diff before `rblxsync run`.
2. **Never commit, print, or paste `ROBLOX_API_KEY` or `ROBLOX_COOKIE`.** They
   live in a gitignored `.env` (local) or CI secrets. Treat them like passwords.
3. **`--config` is a global flag and works on either side of the subcommand.**
   `rblxsync run --config prod.yml` and `rblxsync --config prod.yml run` are
   the same thing.
4. **Creating a badge costs 100 Robux each.** Confirm with the user before a sync
   that adds new badges.
5. **Before renaming a resource, check the entry has an `id:`.** With an id the
   rename is safe. Without one, matching falls back to the name and the rename
   creates a duplicate while the original keeps selling (and a duplicate badge
   costs another 100 Robux). Add the id with `rblxsync import` first, then
   rename. In a `--dry-run` diff, an unexpected "CREATE" for a resource that
   already exists is this mistake.
6. **Confirm before destructive or paid actions**: new badges (Robux), publishing
   places (goes live), any first real `run`.

## Setup workflow (new project)

Run these steps when adding rblxsync to a codebase that doesn't have it yet.

1. **Confirm install.** `rblxsync --version` prints the version (e.g.
   `rblxsync 0.2.3`). If it is absent, install it: `rokit add dig1t/rblxsync@0.2.3`
   (Aftman and Foreman take the same pin), or grab the zip for the machine from
   the Releases page, or `cargo install --path .` from source. Every release ships
   binaries for Linux, Windows, Intel Mac, and Apple Silicon. See
   `references/cli-and-ci.md`.
2. **Create `.env`** with `ROBLOX_API_KEY=...` and ensure `.env` is gitignored.
   Add `ROBLOX_COOKIE=...` *only* if the config will set universe settings (see
   the cookie gotcha below). Never write real secret values yourself. Leave
   placeholders and tell the user to fill them in.
3. **Does the experience already have live resources?** If it has game passes,
   products, or badges already, do NOT hand-write them. Run
   `rblxsync import --universe-id <id>` to pull them into `rblxsync.yml` and the
   lock file with their real ids attached. Add `--badge-id <id>` for each
   disabled badge (Roblox omits those from its listing) and `--place-id <id>`
   for each place beyond the root one. Otherwise write a minimal
   `rblxsync.yml`: only `universe.id` is required. Copy
   `assets/rblxsync.example.yml` and trim it to what the user actually needs.
4. **Validate:** `rblxsync validate` (no API key / network needed).
5. **Preview:** `rblxsync run --dry-run`.
6. **Apply:** `rblxsync run` once the user approves the preview.
7. **Commit** `rblxsync.yml` and `rblxsync-lock.yml` (NOT `.env`). If `output_path`
   is set, commit the generated `Config.luau` too.

## Minimal config

Only `universe.id` is required:

```yaml
universe:
  id: 123456789

game_passes:
  - name: "VIP Pass"
    price: 100
```

After the first `run` that entry gains its id, and rblxsync matches on that from
then on:

```yaml
game_passes:
  - name: "VIP Pass"
    id: 1122334455
    price: 100
```

(The id line is spliced in right after the entry's first line. Key order in the
entry does not matter to the parser.)

For the full field-by-field schema, defaults, and every gotcha, read
`references/config-schema.md`. A complete annotated sample is in
`assets/rblxsync.example.yml`.

## Commands

| Command | What it does |
| --- | --- |
| `rblxsync run [--dry-run]` | Sync universe settings + game passes, products, badges. Default command. Writes `rblxsync-lock.yml` and (if set) `Config.luau`. |
| `rblxsync publish` | Publish `.rbxl` places where `publish: true`. Always publishes (no "save"). Does NOT need the cookie. |
| `rblxsync validate` | Parse + check the YAML (dup names, etc.). No API key, no network. |
| `rblxsync import [--universe-id ID] [--place-id ID]... [--badge-id ID]...` | Pull a live experience into `rblxsync.yml` AND `rblxsync-lock.yml`, ids included. Use this to adopt rblxsync on an existing game. Remote wins on conflicts; the old config is backed up to `rblxsync.old.yml` first. Icons are not imported. |
| `rblxsync export [-o PATH] [--lua]` | One-way snapshot of live resources to a flat Luau/Lua table. Read-only. NOT a config you can feed back in (use `import` for that), and NOT the same shape as `Config.luau`. |

Full command reference, flags, the GitHub Action, environment variables, and
Open Cloud permission scopes are in `references/cli-and-ci.md`.

## The cookie gotcha (very common error)

`rblxsync run` **hard-fails demanding `ROBLOX_COOKIE`** whenever *any* universe
field is set, including `genre` and `max_players`, which are local-only and
never actually call a cookie API. So a config that sets only `genre` still
requires the cookie even though no cookie request is made.

If a user hits "ROBLOX_COOKIE is not set" and doesn't want to provide a cookie,
the fix is to **remove all `universe.*` fields except `id`** from the config (move
name/description/etc. tracking elsewhere). Universe *settings* updates use
`.ROBLOSECURITY` cookie auth against `develop.roblox.com`. An API key alone
cannot update them. See `references/cli-and-ci.md` for how to obtain the cookie
safely.

## Fields that don't sync (surface these, don't silently rely on them)

- **`genre`** and **`max_players`**: tracked in lock/`Config.luau` but **never
  pushed to Roblox** (`max_players` is a per-place setting).
- **Developer Product `is_active`**: parsed but **not** synced; has no effect.
- **Game Pass `is_for_sale`**: this one *is* synced.

Do not type game code against a Developer Product `IsActive` field. The generated
`DeveloperProduct` type has no such field (`{ Id, Name, Description, Price }`).

## Integrating with game code

If the user wants resource IDs available in Luau, set `output_path` (e.g.
`src/shared/Config.luau` or a Rojo-mapped path) and `require` the generated module:

```lua
local Config = require(game.ReplicatedStorage.Shared.Config)
local vipId = Config.GamePasses[1].Id
print(Config.Universe.Name)
```

Wiring it cleanly (lookup by name, Rojo paths, regenerate-on-sync, lock-file
commits) is covered in `references/integration.md`. Read it before editing a
user's existing Luau to consume rblxsync output.

## Adding to CI

A ready-to-use workflow template is in `assets/github-workflow-sync.yml`. Copy it
to `.github/workflows/`, set repo secrets (`ROBLOX_API_KEY`, optionally
`ROBLOX_COOKIE`), and pin the action to a published tag (`@v0.2.3`; there is no
moving `@v1`). The CI gotchas (the `args` input word-splits and cannot contain
spaces; ids written back during a CI run live only on the runner, so create new
resources locally and commit them) are in `references/cli-and-ci.md`.

## When unsure

- Schema / field semantics / defaults → `references/config-schema.md`
- CLI flags, Action inputs, env vars, permission scopes → `references/cli-and-ci.md`
- Consuming output in Luau, lock-file handling, idempotency → `references/integration.md`
- Authoritative upstream docs: the project's `README.md` and `docs/API.md`.
