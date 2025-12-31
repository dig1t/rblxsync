# rbxsync

`rbxsync` is a Rust-based CLI tool and GitHub Action for declaratively managing Roblox experience metadata via the Open Cloud API. It allows you to define your Universe settings, Game Passes, Developer Products, Badges, and Places in a YAML configuration file and sync them to Roblox with a single command.

## Features

- **Declarative Configuration**: Manage all your game metadata in `rbxsync.yml`.
- **Idempotent Sync**: Only updates resources that have changed. Matches by name.
- **Icon Management**: Automatically uploads icons for Game Passes, Products, and Badges if the local file changes (checksum verification).
- **Place Publishing**: Publish `.rbxl` files to specific Place IDs.
- **Export**: Generate a Luau/Lua config file from existing Roblox resources.
- **Auto-Generated Config**: Automatically output a type-safe Luau module with all resource IDs after sync.
- **CI/CD Ready**: Built for GitHub Actions and automated workflows.

## Installation

### Rokit (Recommended)

[Rokit](https://github.com/rojo-rbx/rokit) is the recommended way to install `rbxsync`:

```bash
rokit add dig1t/rbxsync
```

Or add it manually to your `rokit.toml`:

```toml
[tools]
rbxsync = "dig1t/rbxsync@0.1.0"
```

### Aftman

Add to your `aftman.toml`:

```toml
[tools]
rbxsync = "dig1t/rbxsync@0.1.0"
```

Then run:

```bash
aftman install
```

### Foreman

Add to your `foreman.toml`:

```toml
[tools]
rbxsync = { github = "dig1t/rbxsync", version = "0.1.0" }
```

Then run:

```bash
foreman install
```

### From Source

If you prefer to build from source:

```bash
cargo install --path .
```

### GitHub Releases

Download pre-built binaries from the [Releases](https://github.com/dig1t/rbxsync/releases) page.

---

## GitHub Action

Use `rbxsync` directly in your GitHub Actions workflows for automated deployments.

### Basic Usage

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
        uses: dig1t/rbxsync@v1
        with:
          api_key: ${{ secrets.ROBLOX_API_KEY }}
```

### Action Inputs

| Input | Required | Default | Description |
|-------|----------|---------|-------------|
| `api_key` | **Yes** | - | Roblox Open Cloud API Key |
| `command` | No | `run` | Command to run: `run`, `publish`, `validate`, or `export` |
| `config` | No | `rbxsync.yml` | Path to config file |
| `args` | No | - | Additional arguments (e.g., `--dry-run`) |
| `roblox_cookie` | No | - | `.ROBLOSECURITY` cookie (required for universe settings) |

### Examples

#### Sync on Push to Main

```yaml
name: Deploy to Roblox

on:
  push:
    branches: [main]

jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Sync metadata
        uses: dig1t/rbxsync@v1
        with:
          api_key: ${{ secrets.ROBLOX_API_KEY }}
          command: run

      - name: Publish places
        uses: dig1t/rbxsync@v1
        with:
          api_key: ${{ secrets.ROBLOX_API_KEY }}
          command: publish
```

#### With Universe Settings (Requires Cookie)

```yaml
- name: Sync with universe settings
  uses: dig1t/rbxsync@v1
  with:
    api_key: ${{ secrets.ROBLOX_API_KEY }}
    roblox_cookie: ${{ secrets.ROBLOX_COOKIE }}
    command: run
```

#### Dry Run on Pull Requests

```yaml
name: Preview Changes

on:
  pull_request:
    branches: [main]

jobs:
  preview:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Preview sync changes
        uses: dig1t/rbxsync@v1
        with:
          api_key: ${{ secrets.ROBLOX_API_KEY }}
          command: run
          args: --dry-run
```

#### Validate Config

```yaml
- name: Validate rbxsync config
  uses: dig1t/rbxsync@v1
  with:
    api_key: ${{ secrets.ROBLOX_API_KEY }}
    command: validate
```

#### Custom Config Path

```yaml
- name: Sync production config
  uses: dig1t/rbxsync@v1
  with:
    api_key: ${{ secrets.ROBLOX_API_KEY }}
    config: config/production.yml
```

### Setting Up Secrets

1. Go to your GitHub repository → **Settings** → **Secrets and variables** → **Actions**
2. Click **New repository secret**
3. Add the following secrets:
   - `ROBLOX_API_KEY`: Your Open Cloud API key
   - `ROBLOX_COOKIE` (optional): Your `.ROBLOSECURITY` cookie for universe settings

---

## Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `ROBLOX_API_KEY` | **Yes** | Open Cloud API Key with appropriate permissions |
| `ROBLOX_COOKIE` | Conditional | Your `.ROBLOSECURITY` cookie (required only if updating universe settings) |

You can set these in a `.env` file in your project root:
```bash
ROBLOX_API_KEY=your_api_key_here
ROBLOX_COOKIE=your_roblosecurity_cookie_here
```

## Configuration Reference

Create a `rbxsync.yml` file in your project root. Below is a complete reference of all available options.

### Top-Level Settings

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `assets_dir` | string | No | `"assets"` | Directory containing icon files (relative to config file) |
| `creator` | object | Yes* | - | Creator info for asset uploads (*required for uploading icons) |
| `universe` | object | **Yes** | - | Universe configuration |
| `game_passes` | array | No | `[]` | List of Game Pass configurations |
| `developer_products` | array | No | `[]` | List of Developer Product configurations |
| `badges` | array | No | `[]` | List of Badge configurations |
| `places` | array | No | `[]` | List of Place configurations for publishing |
| `badge_payment_source` | string | No | - | If payment is needed to create badges, set to `"user"` to pay from your account or `"group"` to pay from group funds |
| `output_path` | string | No | - | Path to auto-generate a Luau config file after sync |

---

### `assets_dir` — Icon Directory

Specifies the directory where icon image files are located. All icon paths in Game Passes, Developer Products, and Badges are relative to this directory.

```yaml
assets_dir: assets/icons/

game_passes:
  - name: "VIP"
    icon: "vip.png"  # Resolves to: assets/icons/vip.png
```

---

### `creator` — Creator Configuration

**Required** when uploading icons for Game Passes, Developer Products, or Badges. Defines who owns the uploaded assets.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | **Yes** | Your Roblox User ID or Group ID |
| `type` | string | **Yes** | `"user"` or `"group"` |

```yaml
creator:
  id: "12345678"   # Your User ID or Group ID
  type: "user"     # "user" or "group"
```

**Finding your User ID:**
1. Go to your Roblox profile page
2. The number in the URL is your User ID: `roblox.com/users/12345678/profile`

**Finding your Group ID:**
1. Go to your Group's page on Roblox
2. The number in the URL is your Group ID: `roblox.com/groups/12345678`

---

### `output_path` — Auto-Generated Luau Config

Automatically generates a type-safe Luau module containing all your resource IDs after each sync. This is useful for referencing Game Pass IDs, Product IDs, and Badge IDs in your game code.

```yaml
output_path: "src/shared/Config.luau"
```

After running `rbxsync run`, a file like this is generated:

```luau
--!strict
-- Auto-generated by rbxsync. Do not edit manually.
-- This file is regenerated each time `rbxsync run` completes.

export type Universe = {
    Id: number,
    Name: string?,
    Description: string?,
    -- ... other fields
}

export type GamePass = {
    Id: number,
    Name: string,
    Description: string?,
    Price: number?,
    IsForSale: boolean?,
}

-- ... type definitions for DeveloperProduct, Badge

return {
    Universe = {
        Id = 123456789,
        Name = "My Awesome Game",
        MaxPlayers = 50,
    } :: Universe,

    GamePasses = {
        {
            Id = 111111111,
            Name = "VIP Pass",
            Description = "Exclusive access and perks",
            Price = 100,
            IsForSale = true,
        },
    } :: { GamePass },

    DeveloperProducts = { ... } :: { DeveloperProduct },
    Badges = { ... } :: { Badge },
}
```

---

### `universe` — Universe Settings

Configure your experience's metadata. The `id` field is **required**; all other fields are optional.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | number | **Yes** | Your Universe ID |
| `name` | string | No | Display name of the experience |
| `description` | string | No | Experience description |
| `genre` | string | No | Genre category (tracked locally, not updatable via API) |
| `playable_devices` | array | No | List of supported devices |
| `max_players` | number | No | Maximum players per server |
| `private_server_cost` | string | No | Private server pricing |

**Valid `genre` values:**
- `"all"`, `"adventure"`, `"building"`, `"comedy"`, `"fighting"`, `"fps"`, `"horror"`, `"medieval"`, `"military"`, `"naval"`, `"rpg"`, `"scifi"`, `"sports"`, `"townandcity"`, `"western"`

**Valid `playable_devices` values:**
- `"computer"`, `"phone"`, `"tablet"`, `"console"`, `"vr"`

**`private_server_cost` options:**
- `"disabled"` — Private servers are not available
- `"0"` — Free private servers
- `"100"` (or any number) — Cost in Robux for paid private servers

```yaml
universe:
  id: 123456789
  name: "My Awesome Game"
  description: "An epic adventure managed by rbxsync!"
  genre: "adventure"
  playable_devices: ["computer", "phone", "tablet", "console"]
  max_players: 50
  private_server_cost: "100"  # "disabled", "0" for free, or a number for paid
```

> **Note:** Updating universe settings requires the `ROBLOX_COOKIE` environment variable to be set.

---

### `game_passes` — Game Pass Configuration

Define Game Passes for your experience. Each Game Pass is matched by **name** (case-sensitive).

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | **Yes** | Unique name of the Game Pass |
| `description` | string | No | Game Pass description |
| `price` | number | No | Price in Robux |
| `icon` | string | No | Icon filename (relative to `assets_dir`) |
| `is_for_sale` | boolean | No | Whether the Game Pass is available for purchase |

```yaml
game_passes:
  - name: "VIP Pass"
    description: "Exclusive access and perks for VIP members"
    price: 100
    icon: "vip.png"
    is_for_sale: true

  - name: "Double XP"
    description: "Earn double experience points permanently"
    price: 50
    icon: "double_xp.png"
    is_for_sale: true

  - name: "Beta Access"
    description: "Early access to new features"
    icon: "beta.png"
    is_for_sale: false  # Not yet available
```

---

### `developer_products` — Developer Product Configuration

Define Developer Products (one-time purchasable items) for your experience. Matched by **name** (case-sensitive).

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | **Yes** | Unique name of the product |
| `description` | string | No | Product description |
| `price` | number | **Yes** | Price in Robux |
| `icon` | string | No | Icon filename (relative to `assets_dir`) |
| `is_active` | boolean | No | Whether the product is active |

```yaml
developer_products:
  - name: "100 Coins"
    description: "Get 100 in-game coins instantly"
    price: 10
    icon: "coins_100.png"
    is_active: true

  - name: "Speed Boost"
    description: "Double your speed for 5 minutes"
    price: 25
    icon: "speed_boost.png"
    is_active: true

  - name: "Revive"
    description: "Instantly respawn without losing progress"
    price: 15
    icon: "revive.png"
    is_active: true
```

---

### `badges` — Badge Configuration

Define Badges for your experience. Matched by **name** (case-sensitive).

> **Note:** Creating new badges costs **100 Robux each**. Set `badge_payment_source` to specify where funds come from.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | **Yes** | Unique name of the badge |
| `description` | string | No | Badge description |
| `icon` | string | No | Icon filename (relative to `assets_dir`) |
| `is_enabled` | boolean | No | Whether players can earn this badge |

```yaml
badge_payment_source: "user"  # or "group"

badges:
  - name: "Welcome"
    description: "Awarded for joining the game for the first time"
    icon: "welcome.png"
    is_enabled: true

  - name: "First Win"
    description: "Awarded for your first victory!"
    icon: "first_win.png"
    is_enabled: true

  - name: "Speedrunner"
    description: "Complete the game in under 10 minutes"
    icon: "speedrunner.png"
    is_enabled: true
```

---

### `places` — Place Publishing Configuration

Define places to publish when running `rbxsync publish`.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `place_id` | number | **Yes** | The Place ID to publish to |
| `file_path` | string | **Yes** | Path to the `.rbxl` file |
| `publish` | boolean | No | Whether to publish this place (default: `false`) |

```yaml
places:
  - place_id: 1234567890
    file_path: "places/start_place.rbxl"
    publish: true

  - place_id: 9876543210
    file_path: "places/lobby.rbxl"
    publish: true

  - place_id: 5555555555
    file_path: "places/test_place.rbxl"
    publish: false  # Won't be published
```

---

## Complete Example

Here's a full `rbxsync.yml` example with all features:

```yaml
# Directory for icon files
assets_dir: assets/icons/

# Creator configuration (required for icon uploads)
creator:
  id: "12345678"
  type: "user"

# If payment is needed to create badges, set to "user" to pay from your account, or "group" to pay from group funds
badge_payment_source: "user"

# Auto-generate Luau config after sync
output_path: "src/shared/Config.luau"

# Universe settings
universe:
  id: 123456789
  name: "My Awesome Game"
  description: "An epic multiplayer adventure!"
  genre: "adventure"
  playable_devices: ["computer", "phone", "tablet"]
  max_players: 50
  private_server_cost: "disabled"  # "disabled", "0" for free, or a number for paid

# Game Passes
game_passes:
  - name: "VIP Pass"
    description: "Exclusive VIP perks and rewards"
    price: 100
    icon: "vip.png"
    is_for_sale: true

  - name: "Double XP"
    description: "Permanently earn double experience"
    price: 50
    icon: "double_xp.png"
    is_for_sale: true

# Developer Products
developer_products:
  - name: "100 Coins"
    description: "Get 100 in-game coins"
    price: 10
    icon: "coins.png"
    is_active: true

  - name: "Speed Boost"
    description: "5 minutes of double speed"
    price: 25
    icon: "speed.png"
    is_active: true

# Badges
badges:
  - name: "Welcome"
    description: "Thanks for playing!"
    icon: "welcome.png"
    is_enabled: true

  - name: "Champion"
    description: "Won 100 matches"
    icon: "champion.png"
    is_enabled: true

# Places for publishing
places:
  - place_id: 1234567890
    file_path: "build/game.rbxl"
    publish: true
```

---

## Usage

### Sync (Default)
Syncs universe settings and all assets (game passes, products, badges):
```bash
rbxsync
# OR
rbxsync run
```

Use `--dry-run` to preview changes without applying them:
```bash
rbxsync run --dry-run
```

### Publish Places
Publishes `.rbxl` files defined in the `places` section:
```bash
rbxsync publish
```

### Export
Fetch existing resources from Roblox and generate a Luau/Lua config file (useful for migration):
```bash
# Export as Luau (default)
rbxsync export --output Config.luau

# Export as Lua
rbxsync export --output Config.lua --lua

# Custom output path
rbxsync export --output src/shared/GameConfig.luau
```

### Validate
Check if your `rbxsync.yml` is valid:
```bash
rbxsync validate
```

### Custom Config Path
Use a different config file:
```bash
rbxsync --config my-config.yml run
rbxsync -c production.yml publish
```

---

## API Key Scopes

Ensure your API Key has the following permissions:

| Scope | Required For |
|-------|--------------|
| **Universe** Read/Write | Universe settings sync |
| **Game Passes** Read/Write | Game Pass sync |
| **Developer Products** Read/Write | Developer Product sync |
| **Badges** Read/Write | Badge sync |
| **Assets** Write | Uploading icons |
| **Places** Write | Publishing places |

---

## Lock File

`rbxsync` maintains a `rbxsync-lock.yml` file that tracks:
- Resource IDs (Game Pass IDs, Product IDs, Badge IDs)
- Icon file hashes (for change detection)
- Universe settings state

This file should be committed to version control to ensure idempotent syncs across environments.

---

## License

MIT
