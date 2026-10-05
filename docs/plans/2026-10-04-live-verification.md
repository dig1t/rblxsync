# Live verification and partial-run saves

Status: done, except live place settings (waiting on the `universe.place:write` scope).

## Why

Thumbnails, environments, place settings and the Open Cloud universe
settings were only covered by mock tests. A run that failed partway could
also lose what it had already changed: the lock file was saved after a
create but not after an update, and `Config.luau` was written only when the
whole run succeeded.

## Design

- Each sync saves the lock file after every create **and** update, and `run`
  saves it and writes `Config.luau` even when a step fails, then returns the
  error. A rerun picks up where the failed one stopped.
- Lock entries are kept sorted by id, so a committed lock file diffs cleanly.
- A game pass without a `price` is created without one (Roblox refuses
  `price: 0`).

## Steps

- [x] Partial-run saves, sorted lock entries, pass create without a price
- [x] Live run against a throwaway experience: universe settings, passes,
      products, badges (free and paid), home page thumbnails, publish
      (.rbxl and .rbxlx), environments, import (with `--badge-id`), export
- [ ] Live place settings (root place PATCH): waiting on the
      `universe.place:write` scope
- [x] Game page thumbnails: refused at `validate` (Roblox's endpoints manage
      translated sets only), see Findings
- [x] Forced failure partway through badge creation, then a rerun
- [x] Code review findings (below), tests (129), clippy, fmt
- [x] Docs

## Findings

Live runs found these, all fixed unless noted:

- A pass with no `price` was created with `price: 0`, which Roblox refuses.
- Turning private servers off sent `privateServerPriceRobux: null`, which
  Roblox refuses as malformed JSON. A field in the update mask with no value
  in the body clears it.
- The id write-back missed an entry whose `name:` line ends in a comment.
- `publish` posted to a path that doesn't exist (404) and exited 0 on any
  failure. It now uses `/universes/v1/.../versions`, sends `.rbxlx` as XML,
  and fails the command if any place fails.
- `export` wrote developer products with no id or price (the list calls
  them `productId` and `priceInformation`).
- Pass and product icon updates also uploaded the file through the Assets
  API. Roblox never used that asset (the PATCH carries the file and makes
  its own), so each change left an orphan image and needed `creator:` and
  the `asset` scope. The upload is gone and `creator:` is ignored with a
  warning.
- Game page thumbnails: the game-internationalization endpoints answer
  `You can't update translations for source language` (code 26) for the
  experience's source language and accept other languages. The
  source-language set is only reachable with a cookie (develop/publish
  APIs). The endpoints are Roblox's localization API (documented as such,
  with error 26 listed), so `thumbnails.game_page` now fails `validate`; the
  plan/apply code stays for a possible localized-sets feature.
- A badge create with an unreadable icon is a 500 from Roblox, not a 400; no
  badge is made and no quota is used.

Follow-ups on those, done:

- `validate` refuses `price: 0` on passes and products: Roblox refuses it
  on create even for a pass that isn't for sale ("Price must be at least 1",
  "Price cannot be less than or equal to 0"), which would stop a run partway.
  The docs said a pass "defaults to 0"; they now say it's created with no
  price.
- `export` reads pass prices from each pass's detail; the list has none.
- Regression tests: a run that fails partway, `publish` failing after the
  other places, a pass created without a price, `price: 0` refused.
- The orphan images the old icon path left can't be archived through Open
  Cloud ("not an archivable asset type"), so the two from these tests stay in
  the test group's images, as do any older ones (their description starts
  "Uploaded by rblxsync from"). Only the Creator Hub can remove them.

## Code review fixes

A max-effort review of the day's diff found these, all fixed with a test
unless noted:

- `import` kept the old config's devices, private server price and
  `max_players` while `run` diffs against live Roblox, so import then run
  reverted Creator Hub changes (a price change cancels every private server
  subscription). Settings the config manages are now refreshed from Roblox.
- `import` wrote the root place's `server_size` next to `universe.max_players`,
  which preflight refuses. With `max_players` set, the size goes there only.
- The yml editor treated a column-0 `- ` (how serde_yaml, and so `import`,
  writes lists) as the next top-level key, so write-back failed on every
  imported config.
- Write-back on a name match could persist a stale lock id as a permanent
  `id:`. Only ids Roblox confirmed (created, or in its list) are written.
- A textual write-back could leave `rblxsync.yml` unloadable (`ids: { dev: 1, }`
  became `{ dev: 1,, staging: 2 }`). An edit is saved only if the file still
  loads.
- `truncate_body` sliced by bytes and could panic on UTF-8 with debug logs.
- Universe and place settings ran first, so a Team Create 409 on the root
  place blocked every pass, product and badge. They now run after the items.
- `run --env X` accepted two environments on one universe; `resolve` refuses it.
- `validate` never checked an environment's overrides; it now validates each
  environment as it resolves.
- One failed home page upload status read gave up on an upload that already
  exists on Roblox; reads are retried within the wait.
- Smaller: a misplaced doc comment, history-style comments, the `--env` badge
  warning's fix, CRLF files rewritten as LF, `<no id>` in root place logs, and
  doc errors (website scopes, `--env` on `import`, example configs).

Not fixed:

- Without a committed lock file, CI uploads the home page thumbnails again on
  every run and leaves the earlier uploads behind (documented: commit the lock).
- `universe.max_players` used to be local-only and now sets the root place's
  server size. Documented as an upgrade note; the release notes need it too.
