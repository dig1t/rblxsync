---
sidebar_position: 3
title: Quick Start
---

# Quick Start

1. Set your Open Cloud API key. Create a `.env` file in your project root:

   ```bash
   ROBLOX_API_KEY=your_api_key_here
   ```

   > **Security:** `.env` is gitignored and must never be committed. The API key is a secret.

2. Create a minimal `rblxsync.yml` (only `universe.id` is required):

   ```yaml
   universe:
     id: 123456789

   game_passes:
     - name: "VIP Pass"
       price: 100
   ```

3. Sync:

   ```bash
   rblxsync run
   ```

That's it. Add Game Passes, Developer Products, Badges, Places, and universe settings as needed. See the [configuration schema](/api#configuration-schema) for the full schema.

## CLI Commands

The `-c` / `--config` flag is **global** and must come **before** the subcommand (clap parses it on the top-level command, not the subcommand):

```bash
rblxsync --config production.yml run    # correct
rblxsync run --config production.yml    # WRONG - clap will reject this
```

| Command | Description |
|---------|-------------|
| `rblxsync run` | Sync universe settings + assets (Game Passes, Products, Badges). This is the default if no subcommand is given. Add `--dry-run` to preview. |
| `rblxsync publish` | Publish `.rbxl` files defined in the `places` section. Always creates a **Published** version (there is no "Saved" option). |
| `rblxsync export` | Fetch existing resources from Roblox and dump them to a flat Luau/Lua table. See the note below. |
| `rblxsync validate` | Validate `rblxsync.yml` without contacting the API. |

```bash
rblxsync run --dry-run                 # preview changes
rblxsync export --output Config.luau   # default extension
rblxsync export --output Config.lua --lua
```

**About `export`:** it produces a flat, **untyped** table with snake_case keys (`game_passes`, `developer_products`, `badges`; `game_passes` and `developer_products` hold `name`/`id`/`price`, while `badges` hold only `name`/`id`). The `--lua` flag only changes the default output filename; the generated content is byte-identical. This output is a **one-way snapshot for inspection/migration**; it is *not* the typed module produced by `output_path`, and it is *not* a valid `rblxsync.yml` you can pass back into `rblxsync run`.

Full details: [CLI commands](/api#cli-commands).
