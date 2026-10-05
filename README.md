# rblxsync

Your game passes, developer products, badges, and thumbnails live in a text file. You run one command. Roblox updates to match.

No more clicking through the Creator Hub every time you change a price. No more copying IDs into your code by hand.

Docs: https://dig1t.github.io/rblxsync/

## What it looks like

Write this in a file called `rblxsync.yml`:

```yaml
universe:
  id: 123456789

game_passes:
  - name: "VIP Pass"
    price: 100
```

Run this:

```bash
rblxsync run
```

rblxsync checks Roblox for a game pass called "VIP Pass". If it isn't there, it makes one. If it is there, it fixes the price to 100. Either way, you end up with what the file says.

Run it again tomorrow and nothing breaks. It never makes a second copy of the same pass.

## By hand vs with rblxsync

Two game passes? Either way is fine. The gap opens up once you have thirty
products, a test place, and other people touching the game.

| What you need to do | Clicking through the Creator Hub | With rblxsync |
|---|---|---|
| Cut prices on 30 products for a weekend sale | 30 forms, 30 saves. Miss one and it sells at full price all weekend. | Change 30 numbers in one file, run it |
| Put the IDs in your game code | Copy each number off a web page and paste it into Luau. One wrong digit prompts the wrong purchase. | The IDs land in a typed Luau file for you |
| Set up a test universe | Recreate every pass, product, and badge by hand, then juggle two sets of IDs in your code | Add its universe ID under `environments:` in the same file, then `rblxsync run --env dev` |
| Find out who dropped the VIP price last week | There's no history. Go ask people. | `git log -p rblxsync.yml` |
| Undo a bad edit | Remember what the description used to say, retype it, hope | `git revert`, run again |
| Swap the home page thumbnails for an update | Upload each image, point the personalization at the new ones, delete the old ones | Replace the files, run it |
| Add 12 badges for an event | 12 forms. Past the 5 free each day, every one costs 100 Robux, and a typo means a badge you can't take back | 12 entries, checked with `--dry-run`. It makes the free ones and stops before spending a single Robux |
| Show a new teammate how monetization works | Hand over a login and hope | They read one file |

The expensive mistake is the one nobody plans for: making the same thing twice.
A pass created twice keeps selling under both IDs, so your sales numbers split in
half, and Roblox lets you turn a badge off but never delete it. rblxsync matches
on the `id:` it saved last time, and `--dry-run` shows you every create before it
happens.

Your config sits in git next to your code, so a price change is a line in a pull
request someone can review, not a form somebody filled in at 2am.

## API key scopes (the one thing you set up yourself)

rblxsync needs an Open Cloud API key, and the key only works for what you
tick when you make it. A missing scope shows up later as a `403` halfway
through a run, so add all of these up front.

Make the key at [create.roblox.com/dashboard/credentials](https://create.roblox.com/dashboard/credentials).
For each row, click **Add API System**, pick the system, choose your
experience, and tick the operations:

| API system | Operations | What it's for |
|---|---|---|
| `game-pass` | `read`, `write` | Game passes |
| `developer-product` | `read`, `write` | Developer products |
| `legacy-universe.badge` | `write`, `manage-and-spend-robux` | Updating and creating badges (`manage-and-spend-robux` is required even for free ones) |
| `legacy-badge` | `manage` | Badge icons |
| `universe` | `write` | Universe settings: devices and private servers |
| `universe.place` | `write` | Place name, description, and server size |
| `universe-places` | `write` | `rblxsync publish` |
| `universe.thumbnail` | `read`, `write` | Home page thumbnails |

Then, under **Accepted IP Addresses**, add `0.0.0.0/0` (or your own IP) and
save. Copy the key; Roblox only shows it once.

Skip a row only if you never use that feature. The API key is all rblxsync
needs: no browser cookie, no password.

## Start here (about 5 minutes)

### 1. Install it

If your project already uses a tool manager, this is one line:

```bash
rokit add dig1t/rblxsync@0.3.0
```

Aftman and Foreman work too. Add it to `rokit.toml`, `aftman.toml`, or `foreman.toml` and run the install:

```toml
[tools]
rblxsync = "dig1t/rblxsync@0.3.0"
```

No tool manager? Grab the zip for your machine from the [Releases](https://github.com/dig1t/rblxsync/releases) page and put the binary somewhere on your PATH. There's a build for Windows, Linux, Apple Silicon Macs, and Intel Macs.

You can also build it from source with [Rust](https://rustup.rs):

```bash
git clone https://github.com/dig1t/rblxsync
cd rblxsync
cargo install --path .
```

Either way, check it worked:

```bash
rblxsync --version
```

### 2. Get an API key

An API key is a password that lets rblxsync talk to Roblox for you.

Follow [API key scopes](#api-key-scopes-the-one-thing-you-set-up-yourself) above. Every scope in that table, on your experience, then copy the key.

### 3. Put the key in a `.env` file

Make a file named `.env` next to your project:

```bash
ROBLOX_API_KEY=paste_your_key_here
```

Then add `.env` to your `.gitignore`. **Never** put this key on GitHub. Anyone who has it can change your game.

### 4. Find your universe ID

Open your experience on the Creator Hub. The number in the address bar is your universe ID:

```
create.roblox.com/dashboard/creations/experiences/123456789/overview
                                                  ^^^^^^^^^
```

Or open Studio, paste `print(game.GameId)` into the command bar, and press Enter.

### 5. Write your config

Make a file named `rblxsync.yml`:

```yaml
universe:
  id: 123456789   # your number from step 4

game_passes:
  - name: "VIP Pass"
    description: "Fly, glow, and skip the queue"
    price: 100

developer_products:
  - name: "100 Coins"
    price: 25

badges:
  - name: "First Win"
    description: "Won your first round"
```

### 6. See what would happen

```bash
rblxsync run --dry-run
```

This changes nothing. It prints what it *would* do so you can check it first. Always do this the first time.

### 7. Do it for real

```bash
rblxsync run
```

Done. Check the Creator Hub and your stuff is there.

## Already have a game with stuff in it?

Don't retype everything. Pull it down instead:

```bash
rblxsync import --universe-id 123456789
```

This writes a `rblxsync.yml` from whatever is already on Roblox. Your old config gets saved as `rblxsync.old.yml` first, so nothing is lost. Places come in with their names and server sizes. Devices, the private server price, and `max_players` are refreshed from Roblox when your config already sets them, so a run right after an import won't undo a change made in the Creator Hub.

Two things it can't see on its own:

- **Extra places.** Roblox only tells the API about your starting place. Add the rest with `--place-id 987654321`, once per place.
- **Disabled badges.** Roblox leaves them out of the list. Pull each one in with `--badge-id 111222333`.

```bash
rblxsync import --universe-id 123456789 --place-id 987654321 --badge-id 111222333
```

## Getting the IDs into your game code

Add this line to your config:

```yaml
output_path: "src/shared/Config.luau"
```

Every `rblxsync run` writes a typed Luau file there with every ID in it:

```lua
--!strict
-- Auto-generated by rblxsync. Do not edit manually.
-- This file is regenerated each time `rblxsync run` completes.

export type Universe = {
	Id: number,
	Name: string?,
	Description: string?,
	PlayableDevices: {string}?,
	MaxPlayers: number?,
	PrivateServerCost: (number | "disabled")?,
}

export type GamePass = {
	Id: number,
	Name: string,
	Description: string?,
	Price: number?,
	IsForSale: boolean?,
}

export type DeveloperProduct = {
	Id: number,
	Name: string,
	Description: string?,
	Price: number?,
}

export type Badge = {
	Id: number,
	Name: string,
	Description: string?,
	IsEnabled: boolean?,
}

return {
	Universe = {
		Id = 123456789,
		Name = "Zombie Diner Tycoon",
		Description = "Cook burgers. Survive the night shift.",
		PlayableDevices = { "computer", "phone", "tablet" },
		MaxPlayers = 12,
		PrivateServerCost = 100,
	} :: Universe,

	GamePasses = {
		{
			Id = 1122334455,
			Name = "VIP Pass",
			Description = "Skip the queue and get a golden name tag",
			Price = 199,
			IsForSale = true,
		},
		{
			Id = 1122336677,
			Name = "Double Tips",
			Description = "Every customer tips twice as much",
			Price = 149,
			IsForSale = true,
		},
		{
			Id = 1122338899,
			Name = "Golden Spatula",
			Description = "Cook burgers twice as fast",
			Price = 299,
			IsForSale = false,
		},
	} :: { GamePass },

	DeveloperProducts = {
		{
			Id = 2233445566,
			Name = "500 Coins",
			Description = "Starter coin pack",
			Price = 25,
		},
		{
			Id = 2233447788,
			Name = "3000 Coins",
			Description = "Most popular coin pack",
			Price = 100,
		},
		{
			Id = 2233449900,
			Name = "Instant Restock",
			Description = "Refill the fridge without waiting",
			Price = 20,
		},
	} :: { DeveloperProduct },

	Badges = {
		{
			Id = 1234567890123456,
			Name = "First Shift",
			Description = "Survived your first night",
			IsEnabled = true,
		},
		{
			Id = 1234567890987654,
			Name = "Night Owl",
			Description = "Survive ten nights across all your runs",
			IsEnabled = true,
		},
	} :: { Badge },
}
```

Then your game code reads IDs from it instead of hard-coding numbers:

```lua
local Config = require(game.ReplicatedStorage.Shared.Config)

MarketplaceService:PromptGamePassPurchase(player, Config.GamePasses[1].Id)
print(Config.Universe.Name)
```

A few things to know about that file:

- It's `--!strict` and fully typed, so Luau autocompletes it and catches your typos.
- Entries are sorted by ID, so git diffs only show what actually changed.
- Empty sections stay as empty tables instead of vanishing, so indexing them is always safe.
- Don't edit it by hand. It gets rewritten from scratch on every run.

## A dev copy of your game

Most games keep a second universe for testing, with the same passes, products, badges, and places under different IDs. Don't copy your config for it. Add it to the same file under `environments:`:

```yaml
universe:
  id: 123456789            # the live game
  name: "My Game"

environments:
  dev:
    universe:
      id: 987654321        # the dev copy
      name: null           # leave the dev copy's name alone
    output_path: src/shared/ConfigDev.luau

game_passes:
  - name: "VIP Pass"
    id: 1111111            # in the live game
    ids: { dev: 2222222 }  # in the dev copy
    price: 100

places:
  - place_id: 555555555
    ids: { dev: 666666666 }
    file_path: build/lobby.rbxl
    publish: true
```

Then say which copy with `--env`:

```bash
rblxsync run --env dev --dry-run
rblxsync run --env dev
rblxsync publish --env dev
```

Leave out `--env` and you get the live game, same as before.

- Every pass, product, badge, and place is written once. Change a price and each copy gets it the next time you run for that copy.
- `--env dev` uses everything at the top level, except what `environments.dev` sets. A field set to `null` there is left alone in that copy.
- Universe settings carry over too. If the top level sets `universe.name`, give the dev copy its own name or `null`, or a dev run renames it to match.
- `name_prefix: "[dev] "` under the environment puts that in front of the game's name and every place name in that copy, so you can tell the copies apart in Studio. Passes, products, and badges keep their names, since game code looks them up by name.
- An environment has to name its own `universe.id`. rblxsync refuses one that would point at the live game.
- Each entry keeps its ID for each copy under `ids:`. rblxsync writes `ids: { dev: ... }` itself when it makes or finds an entry in the dev copy, the same way it writes `id:` for the live game.
- Each copy gets its own lock file: `rblxsync-lock.yml` for the live game, `rblxsync-lock.dev.yml` for dev.
- Give the copy its own `output_path` when your game code picks IDs by `game.GameId`. Leave it out and both copies write the same file, which suits a build that syncs one copy right before uploading it.

Already made a dev copy with the same items? `rblxsync run --env dev --dry-run` shows what it matches by name. The real run updates them to match your config and writes their IDs into `ids:`. Place IDs go in by hand, because Roblox won't list a universe's places. `import` only pulls the live game: a dev copy's prices would overwrite the entries both copies share.

## The commands

Working on more than one game? Point `--config` at a different file. It works on either side of the command. (A dev copy of the same game goes under [`environments:`](#a-dev-copy-of-your-game) instead.)

```bash
rblxsync run --config production.yml
rblxsync --config production.yml run
```

| Command | What it does |
|---------|--------------|
| `rblxsync run` | Makes Roblox match your config. Add `--dry-run` to preview. This is what runs if you type just `rblxsync`. Badges past the day's 5 free ones need `--allow-paid-badges`. |
| `rblxsync publish` | Uploads your `.rbxl` place files and publishes them live. |
| `rblxsync import` | Pulls what's already on Roblox down into your config. |
| `rblxsync validate` | Checks your YAML for typos. Doesn't touch the internet. |
| `rblxsync export` | Dumps your current passes, products, and badges into a plain Lua table. One-way snapshot for reading, not a config you can feed back in. |

## What goes in `rblxsync.yml`

Only `universe.id` is required. Everything else is optional.

| Section | What it's for |
|---------|---------------|
| `universe` | Your universe ID, plus name, description, devices, max players, private server price. |
| `game_passes` | One entry per game pass. |
| `developer_products` | One entry per developer product. `price` is required here. |
| `badges` | One entry per badge. New badges are free up to 5 a day, then cost **100 Robux** each. |
| `places` | Your places: name, description, players per server, and the file `rblxsync publish` uploads. |
| `thumbnails` | Images for your home page tiles (up to 5). |
| `environments` | Other copies of the game, like a dev universe. See [A dev copy of your game](#a-dev-copy-of-your-game). |
| `assets_dir` | Folder your icon and thumbnail images live in. Defaults to `assets`. |
| `badge_payment_source` | `"user"` or `"group"`. Needed to create badges. The wallet a paid badge comes out of. |
| `output_path` | Where to write the Luau file with all your IDs. |

There's a full working example in [`rblxsync.example.yml`](rblxsync.example.yml), and every single field is listed in [docs/API.md](docs/API.md#configuration-schema).

## How it decides what to update

For each pass, product, or badge:

1. If the entry has an `id:`, it uses that. The name is then just a label, so you can rename it freely.
2. If there's no `id:`, it looks for a match by name, ignoring capitals, and writes the match's `id:` into your `rblxsync.yml`.
3. Still nothing? It creates one, then writes the new `id:` straight back into your `rblxsync.yml`.

With `--env dev`, the ID it uses and writes is the entry's `ids: { dev: ... }`.

That write-back happens the moment the resource is created or found, not at the end. So even if the next step crashes, the ID is safe and the next run picks up where you left off instead of making a duplicate. The lock file is saved after every change too, and a run that stops on an error still writes `Config.luau`.

Places can't be created through the API, so a place needs its ID in the config. rblxsync reads each place first and only sends the settings that changed.

Icons work the same way, using a checksum. rblxsync only re-uploads an icon when the file on your disk actually changed. A pass or product gets its icon in the same call that creates or updates it.

Thumbnails use the checksum too:

```yaml
thumbnails:
  home_page:            # home page tiles; two or more turn on personalization
    - image: "thumbnails/home_a.png"
    - image: "thumbnails/home_b.png"
```

A new or changed image gets uploaded and the old upload deleted. The home page's running personalization gets updated in place, so it keeps its statistics.

## Things that will trip you up

**Badges are free 5 a day, then 100 Robux each.** Roblox gives every game 5 free badges per day (GMT). Before it changes anything, rblxsync reads how many are left today and prints it. It creates badges for free while they last. If a run needs more, it stops before touching anything unless you pass `--allow-paid-badges`. Each badge goes out with the price rblxsync expects, so Roblox refuses it rather than charge a different amount. Set `badge_payment_source` to `"user"` or `"group"` so Roblox knows which wallet a paid badge comes out of.

**`universe.name`, `description`, and `max_players` belong to your starting place.** rblxsync sets them as that place's name, description, and server size. You can set them under `universe:` or on the starting place under `places:`, but only in one spot: a run stops if both set the same field. Earlier versions only wrote `max_players` into `Config.luau`, so check it matches your real server size before you upgrade.

**Changing the private server price cancels every subscription.** Roblox does that, not rblxsync, and it also limits how often you can change the price. rblxsync reads the current price first and only sends `private_server_cost` when it's different.

**rblxsync doesn't set your genre.** Roblox only lets you change it once every 3 months, so pick it yourself in the Creator Hub. A `genre:` left in `rblxsync.yml` is ignored with a warning.

**A place open in Team Create can't be changed.** Roblox refuses place updates during a Team Create session. rblxsync only sends a place's settings when they differ from what Roblox has, so this only comes up after you change one. Close the session and run again.

**`is_active` on developer products does nothing.** rblxsync reads it and ignores it. Game pass `is_for_sale` does work, and a new pass with a price goes on sale unless you set `is_for_sale: false`.

**rblxsync only deletes thumbnails it uploaded.** Delete the others in the Creator Hub if you don't want them. Leaving out the `thumbnails` section entirely leaves your thumbnails alone.

**The images at the top of your game's page aren't synced.** Roblox only lets an API key change translated copies of them (one set per language), never the set everyone sees, so upload those in the Creator Hub. A `thumbnails.game_page` in your config fails `validate`.

**Home page thumbnails use an API Roblox calls experimental.** If Roblox changes it, `home_page` can break before the rest of rblxsync does.

**Run rblxsync from your project's root.** `assets_dir`, `output_path`, and each place's `file_path` are relative to the folder you run it from. The lock file always sits next to the config.

## `rblxsync-lock.yml`

rblxsync writes this file to remember what it did: which IDs it made, which icons and thumbnails it already uploaded. Each environment gets its own, like `rblxsync-lock.dev.yml`.

**Commit it to git.** It's what keeps a run on your laptop and a run in CI from stepping on each other. Don't edit it by hand, it gets overwritten.

## Running it in GitHub Actions

Push to `main`, and your game updates itself:

```yaml
name: Sync Roblox Experience

on:
  push:
    branches: [main]

jobs:
  sync:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Sync Roblox metadata
        uses: dig1t/rblxsync@v0.3.0
        with:
          api_key: ${{ secrets.ROBLOX_API_KEY }}
          command: run
```

Put your key in the repo under **Settings → Secrets and variables → Actions**, named `ROBLOX_API_KEY`.

Pin the version like `@v0.3.0`. There's no `@v1` tag yet, so don't use one.

Whatever the action writes into `rblxsync.yml` and `rblxsync-lock.yml` stays on the runner and is gone when the job ends. Create new passes, products, and badges, and sync new or changed thumbnails, on your own machine, then commit both files. Thumbnails have no name to match by, so CI uploads them again on every run until the committed lock file has them.

| Input | Required | Default | What it is |
|-------|----------|---------|------------|
| `api_key` | Yes | | Your Open Cloud API key. |
| `command` | No | `run` | `run`, `publish`, `validate`, or `export`. |
| `config` | No | `rblxsync.yml` | Path to your config file. |
| `args` | No | `""` | Extra flags, like `--dry-run`. |

One catch with `args`: it gets split on spaces with no way to quote anything. Stick to plain flags like `--dry-run`. Anything with a space in it will break apart.

Syncing a dev copy from CI? Pass `args: --env dev`, for example in a job that runs on your `dev` branch.

## Full reference

[docs/API.md](docs/API.md) has every command, every flag, every config field, the lock file format, the generated Luau shape, and which Roblox endpoints get called.

## Working on rblxsync itself

```bash
cargo build                                # build
cargo test                                 # test
cargo clippy --all-targets -- -D warnings  # lint
cargo fmt                                  # format
cargo run -- validate                      # try it on a config
```

Tests and clippy both have to pass before a PR.

## License

MIT
