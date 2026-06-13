---
sidebar_position: 4
title: GitHub Action
---

# GitHub Action

The action checks out and builds `rblxsync` from source (`cargo build --release`), then runs the requested command.

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
        uses: dig1t/rblxsync@v0.1.0
        with:
          api_key: ${{ secrets.ROBLOX_API_KEY }}
          command: run
```

> Pin the action to a published ref. A `v0.1.0` tag exists; a `@v1` moving major tag is **not** published, so do not reference `@v1` until one is created.

## Action Inputs

| Input | Required | Default | Description |
|-------|----------|---------|-------------|
| `api_key` | **Yes** | – | Roblox Open Cloud API key. |
| `command` | No | `run` | One of `run`, `publish`, `validate`, `export`. |
| `config` | No | `rblxsync.yml` | Path to the config file (passed as the global `--config`). |
| `args` | No | `""` | Extra flags appended to the command, e.g. `--dry-run`. |
| `roblox_cookie` | No | `""` | `.ROBLOSECURITY` cookie (see Environment Variables). |

> **`args` is word-split, not shell-quoted.** The action passes `args` unquoted so multiple flags (e.g. `--dry-run --foo`) split into separate arguments. As a result, **any single argument containing spaces will be broken apart**; there is no quoting mechanism. Use only flag-style arguments without embedded spaces.

```yaml
- name: Preview changes
  uses: dig1t/rblxsync@v0.1.0
  with:
    api_key: ${{ secrets.ROBLOX_API_KEY }}
    command: run
    args: --dry-run

- name: Sync with universe settings (requires cookie)
  uses: dig1t/rblxsync@v0.1.0
  with:
    api_key: ${{ secrets.ROBLOX_API_KEY }}
    roblox_cookie: ${{ secrets.ROBLOX_COOKIE }}
    command: run
```

Store `ROBLOX_API_KEY` (and optionally `ROBLOX_COOKIE`) as repository secrets under **Settings → Secrets and variables → Actions**.

Full input/output reference: [GitHub Action](/api#github-action).
