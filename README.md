# rblxsync

Your game passes, developer products, and badges live in a text file. You run one command. Roblox updates to match.

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
| Set up a test universe | Recreate every pass, product, and badge by hand, then juggle two sets of IDs in your code | Copy the file, change `universe.id`, `rblxsync run --config dev.yml` |
| Find out who dropped the VIP price last week | There's no history. Go ask people. | `git log -p rblxsync.yml` |
| Undo a bad edit | Remember what the description used to say, retype it, hope | `git revert`, run again |
| Add 12 badges for an event | 12 forms and 1200 Robux, and a typo means a badge you can't take back | 12 entries, checked with `--dry-run` before a single Robux moves |
| Show a new teammate how monetization works | Hand over a login and hope | They read one file |

The expensive mistake is the one nobody plans for: making the same thing twice.
A pass created twice keeps selling under both IDs, so your sales numbers split in
half, and Roblox lets you turn a badge off but never delete it. rblxsync matches
on the `id:` it saved last time, and `--dry-run` shows you every create before it
happens.

Your config sits in git next to your code, so a price change is a line in a pull
request someone can review, not a form somebody filled in at 2am.

## Start here (about 5 minutes)

### 1. Install it

If your project already uses a tool manager, this is one line:

```bash
rokit add dig1t/rblxsync@0.2.3
```

Aftman and Foreman work too. Add it to `rokit.toml`, `aftman.toml`, or `foreman.toml` and run the install:

```toml
[tools]
rblxsync = "dig1t/rblxsync@0.2.3"
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

1. Go to [create.roblox.com/dashboard/credentials](https://create.roblox.com/dashboard/credentials)
2. Click **Create API Key**
3. Give it access to your experience, and turn on read + write for **Game Passes**, **Developer Products**, **Badges**, **Assets**, and **Places**
4. Under IP access, put `0.0.0.0/0` if you're not sure what your IP is
5. Save it and copy the key

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

This writes a `rblxsync.yml` from whatever is already on Roblox. Your old config gets saved as `rblxsync.old.yml` first, so nothing is lost.

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
	Genre: string?,
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
		Genre = "adventure",
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

## The commands

Working on more than one game? Point `--config` at a different file. It works on either side of the command.

```bash
rblxsync run --config production.yml
rblxsync --config production.yml run
```

| Command | What it does |
|---------|--------------|
| `rblxsync run` | Makes Roblox match your config. Add `--dry-run` to preview. This is what runs if you type just `rblxsync`. |
| `rblxsync publish` | Uploads your `.rbxl` place files and publishes them live. |
| `rblxsync import` | Pulls what's already on Roblox down into your config. |
| `rblxsync validate` | Checks your YAML for typos. Doesn't touch the internet. |
| `rblxsync export` | Dumps your current passes, products, and badges into a plain Lua table. One-way snapshot for reading, not a config you can feed back in. |

## What goes in `rblxsync.yml`

Only `universe.id` is required. Everything else is optional.

| Section | What it's for |
|---------|---------------|
| `universe` | Your universe ID, plus name, description, genre, devices, max players, private server price. |
| `game_passes` | One entry per game pass. |
| `developer_products` | One entry per developer product. `price` is required here. |
| `badges` | One entry per badge. Each new badge costs you **100 Robux**. |
| `places` | Place files for `rblxsync publish`. |
| `assets_dir` | Folder your icon images live in. Defaults to `assets`. |
| `creator` | Who owns uploaded icons. Only needed if you use icons. |
| `badge_payment_source` | `"user"` or `"group"`. Who pays the 100 Robux for a new badge. |
| `output_path` | Where to write the Luau file with all your IDs. |

There's a full working example in [`rblxsync.example.yml`](rblxsync.example.yml), and every single field is listed in [docs/API.md](docs/API.md#configuration-schema).

## How it decides what to update

For each pass, product, or badge:

1. If the entry has an `id:`, it uses that. The name is then just a label, so you can rename it freely.
2. If there's no `id:`, it looks for a match by name, ignoring capitals.
3. Still nothing? It creates one, then writes the new `id:` straight back into your `rblxsync.yml`.

That write-back happens the moment the resource is created, not at the end. So even if the next step crashes, the ID is safe and the next run picks up where you left off instead of making a duplicate.

Icons work the same way, using a checksum. rblxsync only re-uploads an icon when the file on your disk actually changed.

## Things that will trip you up

**Badges cost 100 Robux each.** Every single one. Set `badge_payment_source` to `"user"` or `"group"` so Roblox knows which wallet to pull from.

**Universe settings need a cookie, not just an API key.** Roblox has no Open Cloud endpoint for changing your game's name or description, so rblxsync signs in with your browser cookie instead. If you set *any* field under `universe` besides `id`, you also need this in `.env`:

```bash
ROBLOX_COOKIE=your_roblosecurity_cookie
```

To get it: log into roblox.com, press F12, go to Application → Cookies, and copy `.ROBLOSECURITY`. Guard it like your password. Anyone with that string is logged in as you.

**The cookie rule is greedy.** It kicks in even for `genre` and `max_players`, which rblxsync only tracks locally and never sends anywhere. Setting just `genre` will still stop and ask for a cookie.

**`genre` and `max_players` never reach Roblox.** They get saved to the lock file and your `Config.luau`, and that's it. Change them in Studio or the Creator Hub.

**`is_active` on developer products does nothing.** rblxsync reads it and ignores it. Game pass `is_for_sale` does work.

**Run rblxsync from the folder that holds `rblxsync-lock.yml`.** The lock file is read from wherever your config lives but written to whatever folder you're standing in. Those being different will confuse it.

## `rblxsync-lock.yml`

rblxsync writes this file to remember what it did: which IDs it made, which icons it already uploaded.

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
        uses: dig1t/rblxsync@v0.2.3
        with:
          api_key: ${{ secrets.ROBLOX_API_KEY }}
          command: run
```

Put your key in the repo under **Settings → Secrets and variables → Actions**, named `ROBLOX_API_KEY`. Add `ROBLOX_COOKIE` too if you're changing universe settings.

Pin the version like `@v0.2.3`. There's no `@v1` tag yet, so don't use one.

| Input | Required | Default | What it is |
|-------|----------|---------|------------|
| `api_key` | Yes | | Your Open Cloud API key. |
| `command` | No | `run` | `run`, `publish`, `validate`, or `export`. |
| `config` | No | `rblxsync.yml` | Path to your config file. |
| `args` | No | `""` | Extra flags, like `--dry-run`. |
| `roblox_cookie` | No | `""` | Your `.ROBLOSECURITY` cookie. |

One catch with `args`: it gets split on spaces with no way to quote anything. Stick to plain flags like `--dry-run`. Anything with a space in it will break apart.

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
