# Experience thumbnails

Status: done, uncommitted (waiting on review and a live run with a valid API key)

## Goal

Manage an experience's thumbnails from `rblxsync.yml`, using the two Open Cloud
API families Roblox documents for them:

- **Game page** (the Experience Detail Page carousel, up to 10 images):
  `legacy-game-internationalization/v1/game-thumbnails/...` (API key scope
  `legacy-universe:manage`), listed through the public
  `gameinternationalization.roblox.com/v1/game-thumbnails/games/{id}/images`.
- **Home page** (the tiles, 1-5 active, 2+ personalized):
  `thumbnail-personalization-api/v1/universes/{id}/...` (scopes
  `universe.thumbnail:read` / `universe.thumbnail:write`, experimental).

## Config

```yaml
thumbnails:
  game_page:
    - image: thumbnails/lobby.png   # relative to assets_dir
      alt_text: "Players in the lobby"
  home_page:
    - image: thumbnails/home-a.png
    - image: thumbnails/home-b.png
```

An empty or missing list leaves that set alone on Roblox.

## Lock

`thumbnails.game_page` / `thumbnails.home_page`: one `{image, id, hash}` per
upload rblxsync made, in config order. Matching is by file SHA-256, so moving
or renaming a file doesn't re-upload it.

## Behavior

- An image whose hash matches an earlier upload that is still on Roblox is kept.
  Anything else is uploaded. An upload that drops out of the config, or whose
  file changed, is deleted.
- Thumbnails rblxsync didn't upload are never deleted. On the game page they
  stay after the configured ones and a warning lists them.
- Game page: delete stale uploads, upload new ones, set alt text where it
  differs from Roblox, then order. Order is configured images first, then
  unmanaged ones. It is sent after any upload, since where Roblox puts a new
  image isn't documented.
- Home page: upload and wait on each upload's status, then update the active
  personalization (keeps its statistics) or create one, then delete stale
  uploads. Rejected and spammy thumbnails get a warning.
- Planning (hashes, listings, the 10-image cap including unmanaged images)
  runs right after preflight, before anything changes.
- The language code is the experience's source language
  (`/v1/source-language/games/{id}`). Its set in the listing is the game page
  set (checked against `/v2/games/{id}/media` on live games).

## Findings

- Upload part name is `gameThumbnailRequest.files`, not the spec's `Files`;
  two independent clients use it. `mediaAssetId` comes back as a string.
- The public listing is `cache-control: no-cache` and shows non-approved
  images (a `Rejected` one was seen anonymously), so an id missing from it was
  deleted, and rblxsync re-uploads that image.
- `UploadStatus` is an undocumented int enum (1, 2). Completion is read from
  `uploadThumbnailStatusDict` holding the operation id.
- The 10 cap is on images. Of 300 sampled live games, five show 10 images
  plus a preview video in `/v2/games/{id}/media`, so the video isn't counted.
- A dry run against live Roblox read the public game page endpoints correctly
  (Brookhaven: 9 images in page order). The home page endpoints are untested
  live: the API key in the local `.env` gets `401 Invalid API Key` from every
  apis.roblox.com call, including the existing developer products listing.

## Steps

- [x] Config + lock types
- [x] API client methods (`src/api/thumbnails.rs`)
- [x] Plan/apply logic (`src/thumbnails.rs`), hooked into `validate`, `run`,
      `import`
- [x] Tests (wiremock): upload/keep/delete/order/alt text, personalization
      create/update/skip, cap check, dry run makes no writes
- [x] Docs: README, docs/API.md, website, bundled skill, example configs,
      AGENTS.md
- [x] `cargo test` (97 pass), `cargo clippy --all-targets -- -D warnings`, `cargo fmt`, docs site build

## Not done

- `import` doesn't pull existing thumbnails into the config (there's no local
  file to point at). It carries the config section and lock entries over
  untouched.
- Localized (per-language) thumbnail sets, video thumbnails, and the game icon.
- CI-only setups: thumbnails have no name to match by, so the lock file has to
  be committed after a local sync, or CI uploads them again (documented).
