# Import Command Design

## Goal

Add `rblxsync import`: pull an existing experience's metadata down from Roblox
into local `rblxsync.yml` **and** `rblxsync-lock.yml`. The use cases:

1. A team with an existing, launched experience wants to start using rblxsync —
   import bootstraps their config + lockfile from what's already live.
2. A team already using rblxsync later has a resource (e.g. a game pass) that
   rblxsync doesn't know about — import adopts it into both files.

Import is **destructive on conflicts**: remote is authoritative and overwrites
matching local values. It is **additive** for things only present locally.

## Command

```
rblxsync import [--universe-id <id>] [-c <config path>]
```

- Universe ID resolution: `--universe-id` flag if given, else `universe.id` from
  an existing `rblxsync.yml`. Error if neither is available.
- Requires `ROBLOX_API_KEY`. Does **not** require `ROBLOX_COOKIE` (universe
  read uses the Open Cloud `cloud/v2` GET, not the cookie configuration API).

## What gets imported

| Resource | Source | Notes |
|---|---|---|
| Universe name, description | `GET cloud/v2/universes/{id}` | only name + description; see note below |
| Game passes | list + per-pass detail | name, description, price, is_for_sale |
| Developer products | list (creator endpoint) | name, description, price |
| Badges | list | name, description, is_enabled (badges list already returns full objects) |
| Places | root place auto + `--place-id` flag | see note below; written with placeholder `file_path: ""`, `publish: false` |
| Icons | — | **not imported** (can't download to a local file) |

"Fetch full details" means: where a list endpoint omits `description` /
`is_for_sale`, do a per-resource GET so a later `run` won't overwrite the remote
description with a blank. Exact detail endpoints/shapes verified during
implementation.

### Universe genre / playable_devices / private_server_cost / max_players

These are **out of scope for import** — rblxsync can't write them yet (Roblox's
API doesn't fully support it). Import never fetches them. If an existing
`rblxsync.yml` already sets them, they are carried over verbatim into the
rewritten yml so import doesn't clobber the user's values; otherwise they're
simply absent.

## Stable IDs / rename support

Today resources are matched by **display name**, so the name *is* the identity —
renaming a resource in `rblxsync.yml` makes `run` create a duplicate instead of
renaming. Fix: give each entry an optional stable `id`.

- Add `id: Option<u64>` to `GamePassConfig`, `DeveloperProductConfig`,
  `BadgeConfig` (places already have `place_id`).
  `#[serde(skip_serializing_if = "Option::is_none")]` so hand-authored ymls stay
  clean until an id is present.
- `run` match precedence per entry: **`id` if set** → lockfile by name → remote
  by name → create.
  - `id` set and found in lockfile (keyed by id) → compare attributes, PATCH
    (display name is now just a mutable attribute → rename is safe).
  - `id` set but not in lockfile → adopt that id directly (PATCH), don't create.
- `import` writes the `id` for every imported resource (part of its full yml
  write).
- `run` write-back: when it **creates** a new resource whose yml entry had no
  `id`, it inserts `id: <new_id>` into that entry via a **surgical line-insert**
  into the existing yml text — preserving comments, formatting, and every other
  entry. No full re-serialize, no `.old` backup on routine runs. If the entry
  can't be located for the insert, log a warning to run `import` to backfill ids.

## Reconciliation rules

Applied per resource type, matching **by name** (case-insensitive), same as `run`:

- On remote → write remote values to both yml and lockfile (overwrite local).
- In yml but not remote → keep the local entry untouched.
- In lockfile only (not yml, not remote) → drop (stale state).

Places match by `place_id`: a remote place already present locally keeps its real
`file_path`; a remote place not present locally is added with the placeholder.

### Place discovery (verified against the Open Cloud OpenAPI spec)

With an **API key only**, Roblox exposes no endpoint to enumerate a universe's
places — the one list endpoint (`GET develop.roblox.com/v1/universes/{id}/places`)
lives on `develop.roblox.com` and needs the `.ROBLOSECURITY` cookie. So import:
- auto-discovers the **root place** from the universe's `rootPlace` field, and
- accepts a repeatable `--place-id <id>` flag for any additional places. Each is
  validated + named via the universe-scoped `GET cloud/v2/universes/{id}/places/{place_id}`
  (a wrong id/universe → 404 → warn and skip, don't abort).

`.rbxl` files still can't be downloaded, so `file_path` is always the placeholder.

## Icons

No icon path is written and no `icon_hash` / `icon_asset_id` is stored on import.
Consequence: if the user later adds an `icon:` path to a resource, the next
`run` sees no stored hash and uploads it — exactly the desired "add an icon
later" flow, for free.

## File handling

1. Load existing `rblxsync.yml` (if any) and `rblxsync-lock.yml` (if any) so
   local-only entries can be preserved.
2. Fetch remote data.
3. Build merged `RblxSyncConfig` and `SyncState` per the rules above.
4. If `rblxsync.yml` exists, rename it to `rblxsync.old.yml`; if that exists,
   `rblxsync.old1.yml`, `.old2.yml`, … (never overwrite a backup).
5. Write the new `rblxsync.yml` (via `serde_yaml`) and `rblxsync-lock.yml`.

Comments/formatting in the old yml are not preserved — the original is retained
as the `.old` backup.

## Code shape

- `src/main.rs`: add `Import { universe_id: Option<u64> }` subcommand; dispatch
  to `commands::import`. Import does not need the cookie client.
- `src/api/mod.rs`: add read methods as needed — `get_universe`,
  `get_game_pass`, `get_developer_product` (badges/products may already carry
  enough in their list responses), `list_places`.
- `src/commands.rs`:
  - add `pub async fn import(...)`. Reuses `RblxSyncConfig` / `SyncState` and
    their existing `update_*` setters; adds a backup-rename helper.
  - update `sync_game_passes` / `sync_developer_products` / `sync_badges` for
    id-first matching and the create-time id write-back (surgical yml insert).
- `src/config.rs`: add `id: Option<u64>` to the three resource configs
  (`skip_serializing_if` none). `RblxSyncConfig` already derives `Serialize`, so
  import can write the whole file back as-is.
- `src/state.rs`: no new types expected (lockfile already keyed by id).

## Out of scope (YAGNI)

- Selective import (`import game-passes` only): re-running full `import` is an
  idempotent merge that already picks up a single missing resource. Skip unless
  asked.
- Downloading place files or icons.
- A `--dry-run` for import (it backs up the old yml anyway, so it's recoverable).

## Verification

- Unit tests with `wiremock` (mirroring existing `commands.rs` tests):
  - remote entry overwrites a differing local entry in both yml + lock
  - local-only yml entry is preserved
  - lockfile-only stale entry is dropped
  - place IDs imported with placeholder path; existing place file_path preserved
  - backup rename picks `.old`, then `.old1`, when files already exist
  - import writes `id` into the yml for game passes / dev products / badges
- Rename support tests:
  - `run` with an `id` set + changed name → PATCH (rename), no duplicate create
  - `run` with `id` set but absent from lockfile → adopts (PATCH), no create
  - `run` create writes `id` back into the yml entry, preserving surrounding
    comments/other entries
- `cargo test` and `cargo clippy --all-targets -- -D warnings` clean.
