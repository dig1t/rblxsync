# rbxsync

A Rust-based CLI and GitHub Action for interacting with the Roblox Cloud API.

## Features

- Built with Rust for performance and safety.
- Uses `clap` for command-line argument parsing.
- Uses `reqwest` for HTTP requests.
- Async runtime with `tokio`.
- Configurable via environment variables or `.env` file.

## Setup

1. **Install Rust**: Ensure you have Rust installed via [rustup](https://rustup.rs/).
2. **Environment Variables**:
   Create a `.env` file in the root directory:
   ```env
   ROBLOX_API_KEY=your_api_key_here
   ROBLOX_UNIVERSE_ID=your_universe_id_here
   ```

## Usage

### CLI

Run the tool using `cargo`:

```bash
# List datastores
cargo run -- list-datastores

# List datastores with limit
cargo run -- list-datastores --limit 10
```

### GitHub Action

You can use this repository as a GitHub Action in your workflows.

```yaml
steps:
  - uses: actions/checkout@v4
  
  - uses: ./ # Or your-username/rbxsync@main
    with:
      api_key: ${{ secrets.ROBLOX_API_KEY }}
      universe_id: '123456789'
      command: 'list-datastores'
      args: '--limit 5'
```

## Development

- `cargo build`: Build the project.
- `cargo test`: Run tests.
- `cargo fmt`: Format code.
- `cargo clippy`: Lint code.

## License

MIT
