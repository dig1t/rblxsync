# Environments and place settings

Status: done, together with thumbnails (`2026-10-04-thumbnails.md`).

## Why

A game usually has a live universe and a dev copy. Each has its own passes,
products, badges and places, so the ids differ while everything else stays
the same. Until now that meant a second, full `rblxsync.yml` per universe
(say `sync/dev/rblxsync.yml`, with its own lock file), and every change had
to land in both files.

Subplaces had no settings at all: `places:` only published files, and
`import` wrote each place as a bare id.

## Design

- `environments.<name>` in `rblxsync.yml` overrides top-level keys for
  `--env <name>`: mappings merge key by key, anything else replaces, and an
  explicit `null` removes the key so the field falls back to its default
  (for `universe.name` or `thumbnails`, that means unmanaged). It must set its
  own `universe.id`. It can't list `game_passes`, `developer_products`,
  `badges` or `places`: entries are shared.
- `environments.<name>.name_prefix` (environments only) goes in front of the
  experience's and every place's name in that copy ("[dev] Lobby").
  Item names never get it: game code looks items up by name.
- Each entry keeps its id per environment in `ids: { <env>: <id> }` (`id` /
  `place_id` stay the top level's). `--env` swaps those in before syncing, so
  the sync code is unchanged. With no id for that environment, an entry is
  matched by name in that universe, as at the top level.
- `run` writes the id back when it creates **or adopts** an entry (adopting
  by name used to leave the yml without one, so a later rename made a
  duplicate). With `--env` it writes `ids.<env>`.
- Lock file per environment: `rblxsync-lock.<env>.yml` beside the config. The
  thumbnail plan carries the env so its saves go to the same file.
- `places[]` gains `name`, `description` and `server_size`, synced with
  `PATCH /cloud/v2/universes/{u}/places/{p}` (API key, `universe.place:write`).
  Each place is read first and only changed fields are sent: Roblox answers
  409 while a place is open in Team Create, so a no-op PATCH could fail a run.
  The root place's name and description are the experience's; preflight
  refuses a root place that sets either while `universe:` sets it too. (So
  the root place is also a cookie-free way to set the experience name.)
- `import` reads every place it finds and writes its name, description and
  server size (the root place gets only its server size). `import --env` is
  refused: dev values would overwrite the shared entries. `run --env`
  adopts by name instead.
- Thumbnails are per universe only (no per-place endpoint), so subplace
  thumbnails aren't possible; universe thumbnails are the thumbnails plan.

## Steps

- [x] Config: `environments`, `ids`, place fields, `resolve()`, validation
- [x] Per-environment lock file and `--env` flag on every command
- [x] Id write-back into `ids.<env>`, and on adopt
- [x] Place settings sync and root-place preflight check
- [x] Import pulls place names, descriptions and server sizes
- [x] `name_prefix` for environment copies
- [x] Tests, clippy, fmt (117 tests)
- [x] Docs: README, docs/API.md, website, example configs, skill, AGENTS.md
- [x] Read-only checks against real universes: `run --env dev --dry-run`
      (name match in the dev universe, place SKIP/UPDATE/no-id skip) and
      `import` of a dev universe (subplace names, server sizes)
- [x] Merged with thumbnails in the main checkout
- [x] Live `run --env` against a throwaway experience: name matches, `ids`
      write-back (flow map, next to an existing `id:`), its own lock file and
      output, then a no-op rerun
- [ ] Not verified live: an actual place PATCH. The test key lacks
      `universe.place:write`; the request shape is covered by a mock test.
- [ ] Release, then fold games that keep a second config into one (below)

## Folding a second config into one (after a release)

`rblxsync.yml` keeps everything it has now, gains the dev universe, and each
entry gets its dev id from the old dev config (`sync/dev/rblxsync.yml`):

```yaml
# Only the new lines: every entry keeps its other fields (price, icon, ...).
environments:
  dev:
    universe:
      id: 2222222222
      name: null          # keep the dev copy's own name
      description: null   # and its own description
    output_path: src/Shared/GeneratedDev.luau
    name_prefix: "[dev] " # matches the current dev place names

game_passes:
  - name: VIP
    ids: { dev: 3333333333 }
developer_products:
  - name: 100 Coins
    ids: { dev: 4444444444 }
badges:
  - name: First Win
    ids: { dev: 5555555555 }

places:
  - place_id: 1111111111            # the root place
    ids: { dev: 6666666666 }
    server_size: 24
  - name: Arena                     # dev only until the live place exists
    ids: { dev: 7777777777 }
    server_size: 24
```

Then: `git mv sync/dev/rblxsync-lock.yml rblxsync-lock.dev.yml`, delete
`sync/dev/`, run `rblxsync run --env dev --dry-run` (everything should SKIP),
and point any comment or doc that names `sync/dev/rblxsync.yml` at `--env dev`.
