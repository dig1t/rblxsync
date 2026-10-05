---
name: rblxsync
description: >-
  Declaratively manage Roblox experience metadata (Universe settings, Game
  Passes, Developer Products, Badges, Thumbnails, Places) from a YAML file via
  the rblxsync CLI / GitHub Action. Use when setting up rblxsync in a Roblox
  project, writing or editing rblxsync.yml, syncing/publishing game passes,
  developer products, badges, home page thumbnails (thumbnail
  personalization), places or universe settings, wiring the generated Config.luau into
  Luau game code, debugging ROBLOX_API_KEY errors, or adding
  rblxsync to CI, or when adopting rblxsync on a game that already has live
  monetization. Triggers on "rblxsync", "rblxsync.yml", "sync game passes",
  "Open Cloud metadata", "publish place", "Config.luau", "rblxsync-lock.yml",
  "rblxsync import".
---

# rblxsync

`rblxsync` is a Rust CLI + GitHub Action that **declaratively** manages Roblox
experience metadata via the Open Cloud API. One YAML file (`rblxsync.yml`) is the
source of truth for Universe settings, Game Passes, Developer Products, Badges,
Thumbnails, and Places. Running it is idempotent: resources are matched by **id** when the
config entry has one, otherwise by **name** (case-insensitive), created if
missing, updated (PATCH) if present.

Use this skill to set rblxsync up in a user's Roblox project, author/edit their
config, run syncs safely, and wire the generated `Config.luau` into game code.

## Mental model (read this first)

- **`rblxsync.yml`** = desired state, hand-edited, committed.
- **`rblxsync-lock.yml`** = generated state (resource IDs, icon hashes, and
  the thumbnails rblxsync uploaded).
  **Commit it.** Never hand-edit it: it is overwritten on the next sync.
- **`Config.luau`** (only if `output_path` is set) = generated, typed Luau module
  of all resource IDs. Game code `require`s it. Never hand-edit it.
- **Matching is id-first.** Each `game_passes[]`, `developer_products[]`, and
  `badges[]` entry takes an optional `id:`. Resolution order on `run`:
  1. Entry has `id:` -> matched by that id. The `name` is only a label.
  2. No `id:` -> matched by name, case-insensitive, and the match's `id:` is
     written back into the user's `rblxsync.yml`.
  3. No match -> created, and the new `id:` is written back into the user's
     `rblxsync.yml` right away (a line insert that keeps their comments and
     formatting), so a later failure can't lose it and the next run adopts the
     resource instead of duplicating it.
- **Renaming is safe once an entry has its `id:`**, which is the normal state
  after the first successful run. Change the `name:` and rblxsync PATCHes the
  existing resource.
- **An entry with no `id:` is the one to watch.** Renaming that one means no
  match by the new name, so rblxsync creates a second resource and leaves the
  original selling. A duplicate badge also uses up one of the day's 5 free
  badges, or costs 100 Robux once they're gone.
  - Fix: give the entry its id rather than warning the user off the rename.
    Run `rblxsync import` (or `import --badge-id <id>` for a disabled badge) to
    pull the real id into the config, then rename freely.
  - Do not hand-edit `rblxsync-lock.yml` to preserve an id. That was the old
    workaround and it is no longer the answer.
  - `--dry-run` still proves it: an unexpected "CREATE" for something that
    already exists means the entry is missing its id.
- **Environments = other copies of the same game** (a dev universe) in the same
  file. `environments.<name>` overrides top-level keys (it must set its own
  `universe.id`; `null` clears an inherited field), and every entry keeps its id
  in that copy under `ids: { <name>: <id> }`. `--env <name>` on any command
  but `import` syncs that copy, with its own lock file `rblxsync-lock.<name>.yml`. Never
  suggest a second config file per universe; that is what this replaces.
- **Places** take `name`, `description`, and `server_size`, synced on `run`
  through Open Cloud (only changed fields; Roblox refuses while the place is
  open in Team Create). Places can't be created: each needs `place_id` (or
  `ids.<name>`). The root place's name/description are the experience's, so set
  them in `universe:` or on the root place, never both.

## Golden rules

1. **Never run a mutating sync without previewing first.** Always do
   `rblxsync run --dry-run` and show the user the diff before `rblxsync run`.
2. **Never commit, print, or paste `ROBLOX_API_KEY`.** It lives in a
   gitignored `.env` (local) or a CI secret. Treat it like a password.
3. **`--config` and `--env` are global flags and work on either side of the
   subcommand.** `rblxsync run --env dev` and `rblxsync --env dev run` are the
   same thing. Check which universe a command will touch before running it.
4. **Badges are free 5 a day per game (GMT), then 100 Robux each.** `run` reads
   and prints the day's free quota before changing anything, and stops before a
   badge past it unless given `--allow-paid-badges`. Never pass that flag
   without the user's explicit go-ahead for the Robux, and confirm with the user
   before a sync that adds new badges.
5. **Before renaming a resource, check the entry has an `id:`.** With an id the
   rename is safe. Without one, matching falls back to the name and the rename
   creates a duplicate while the original keeps selling (and a duplicate badge
   burns a free slot or 100 Robux). Add the id with `rblxsync import` first, then
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
   Never write real secret values yourself. Leave placeholders and tell the
   user to fill them in.
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
| `rblxsync run [--dry-run] [--allow-paid-badges]` | Sync universe settings + game passes, products, badges, place settings, thumbnails. Default command. Writes `rblxsync-lock.yml` (`rblxsync-lock.<env>.yml` with `--env`) and (if set) `Config.luau`. `--allow-paid-badges` lets badges past the day's free quota cost 100 Robux each. |
| `rblxsync publish` | Publish `.rbxl` places where `publish: true`. Always publishes (no "save"). |
| `rblxsync validate` | Parse + check the YAML (dup names, etc.). No API key, no network. |
| `rblxsync import [--universe-id ID] [--place-id ID]... [--badge-id ID]...` | Pull a live experience into `rblxsync.yml` AND `rblxsync-lock.yml`, ids included. Use this to adopt rblxsync on an existing game. Remote wins on conflicts; the old config is backed up to `rblxsync.old.yml` first. Places come with name, description, server size. Icons and thumbnails are not imported (a `thumbnails` section carries over as is). Top level only: it refuses `--env` (use `run --env <name>`, which adopts by name). |
| `rblxsync export [-o PATH] [--lua]` | One-way snapshot of live resources to a flat Luau/Lua table. Read-only. NOT a config you can feed back in (use `import` for that), and NOT the same shape as `Config.luau`. |

Full command reference, flags, the GitHub Action, environment variables, and
Open Cloud permission scopes are in `references/cli-and-ci.md`.

## Universe settings

Everything runs on the API key. `name`, `description`, and `max_players` go to
the root place (displayName / description / serverSize); `playable_devices` and
`private_server_cost` go to the universe. `run` reads Roblox first and sends
only fields that differ. Never force-resend a private server price: a change
cancels every private server subscription and Roblox rate-limits it. Don't set
a root place's name, description, or `server_size` under both `universe:` and
`places:`; preflight stops the run.

`genre` is not synced. Roblox allows a genre change only once every 3 months, so
the user sets it in the Creator Hub. A `genre:` in the config is ignored with a
warning.

## Fields that don't sync (surface these, don't silently rely on them)

- **Developer Product `is_active`**: parsed but **not** synced; has no effect.
- **Game Pass `is_for_sale`**: this one *is* synced. Left out, a new pass with a
  `price` is created on sale.

## Thumbnails

`thumbnails.home_page` (up to 5; 2+ turn on Roblox's thumbnail personalization)
lists image files in `assets_dir`. `thumbnails.game_page` fails `validate`: the
game page endpoints an API key reaches are Roblox's localization API, which only
manages translated sets and refuses the source language (the set everyone sees).
Point the user at the Creator Hub for game page images.

- An image uploads only when its file is new or changed (SHA-256 vs the lock
  file). Renaming or moving a file doesn't re-upload it.
- rblxsync deletes a thumbnail it uploaded once its entry is removed or its file
  changes. It never deletes thumbnails it didn't upload. Tell the user, don't
  delete them for them.
- `home_page` updates the running personalization in place (keeps its
  statistics) or starts one. That API is experimental on Roblox's side.
- A missing or empty list leaves that set alone; removing it deletes nothing.
- Permissions: `universe.thumbnail:read` + `universe.thumbnail:write`.
- Show the user the `--dry-run` lines (`[UPLOAD]`, `[DELETE]`, `[UPDATE]`)
  before a real run that deletes thumbnails.

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
to `.github/workflows/`, set repo secrets (`ROBLOX_API_KEY`), and pin the action to a published tag (`@v0.2.3`; there is no
moving `@v1`). The CI gotchas (the `args` input word-splits and cannot contain
spaces; ids and thumbnail uploads written back during a CI run live only on the
runner, so create new resources and sync new or changed thumbnails locally and
commit both files) are in `references/cli-and-ci.md`.

## When unsure

- Schema / field semantics / defaults → `references/config-schema.md`
- CLI flags, Action inputs, env vars, permission scopes → `references/cli-and-ci.md`
- Consuming output in Luau, lock-file handling, idempotency → `references/integration.md`
- Authoritative upstream docs: the project's `README.md` and `docs/API.md`.
