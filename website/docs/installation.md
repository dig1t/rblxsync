---
sidebar_position: 2
title: Installation
---

# Installation

> **Note:** The tool-manager and release-binary installs below assume published release artifacts. At the time of writing the repository publishes the GitHub Action (which builds from source) and the `v0.1.0` source tag; there is no binary-release pipeline yet. Until pre-built binaries are published, prefer **From Source**.

## From Source

This is the recommended path until pre-built binaries are published.

```bash
cargo install --path .
```

## Rokit / Aftman / Foreman

Once binary releases are published, you can install via a tool manager. Pin to a published tag (currently `0.1.0`):

```toml
# rokit.toml or aftman.toml
[tools]
rblxsync = "dig1t/rblxsync@0.1.0"
```

```bash
rokit add dig1t/rblxsync@0.1.0   # Rokit
aftman install                   # Aftman
foreman install                  # Foreman
```

## GitHub Releases

Pre-built binaries, if available, are published on the [Releases](https://github.com/dig1t/rblxsync/releases) page.
