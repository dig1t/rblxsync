# rbxsync

`rbxsync` is a Rust-based CLI tool and GitHub Action for declaratively managing Roblox experience metadata via the Open Cloud API. It allows you to define your Universe settings, Game Passes, Developer Products, Badges, and Places in a YAML configuration file and sync them to Roblox with a single command.

## Features

- **Declarative Configuration**: Manage all your game metadata in `rbxsync.yml`.
- **Idempotent Sync**: Only updates resources that have changed. Matches by name.
- **Icon Management**: Automatically uploads icons for Game Passes, Products, and Badges if the local file changes (checksum verification).
- **Place Publishing**: Publish `.rbxl` files to specific Place IDs.
- **Export**: Generate a Luau/Lua config file from existing Roblox resources.
- **CI/CD Ready**: Built for GitHub Actions and automated workflows.

## Installation

### From Source
```bash
cargo install --path .
```

## Configuration

1. Create a `rbxsync.yml` file in your project root:
   ```yaml
   assets_dir: assets/icons/
   universe:
     id: 123456789  # Your Universe ID (required)
     name: "My Awesome Game"
     description: "Managed by rbxsync"
     genre: "Adventure"
     playable_devices: ["Computer", "Phone"]
     max_players: 20
     private_server_cost: 100  # 0 = free, number = Robux cost, "disabled" = no private servers
   
   game_passes:
     - name: "VIP"
       price: 100
       icon: "vip.png"
   ```

2. Set your Environment Variables:
   - `ROBLOX_API_KEY`: An Open Cloud API Key with permissions for Universe, Game Passes, Badges, and Assets.

## Usage

### Sync (Default)
Syncs universe settings and all assets (game passes, products, badges).
```bash
rbxsync
# OR
rbxsync run
```

### Publish Places
Publishes `.rbxl` files defined in the `places` section of your config.
```bash
rbxsync publish
```

### Export
Fetch existing resources and generate a config file (useful for migration).
```bash
rbxsync export --output config.luau
```

### Validate
Check if your `rbxsync.yml` is valid.
```bash
rbxsync validate
```

## API Key Scopes

Ensure your API Key has the following permissions:
- **Universe**: Read/Write
- **Game Passes**: Read/Write
- **Developer Products**: Read/Write (if using products)
- **Badges**: Read/Write (if using badges)
- **Assets**: Write (for uploading icons)
- **Places**: Write (for publishing)

## License

MIT
