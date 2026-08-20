---
sidebar_position: 4
title: GitHub Action
---

# GitHub Action

Push to `main`, and your game updates itself. No running anything by hand.

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

The action clones rblxsync, builds it, and runs the command you asked for. The first run takes a few minutes because of the build. After that the cargo cache makes it quick.

## Add your secrets

Your API key can't go in the workflow file, it goes in the repo's secret store.

Open your repo, then **Settings → Secrets and variables → Actions → New repository secret**. Add:

- `ROBLOX_API_KEY`
- `ROBLOX_COOKIE`, but only if you're changing universe settings

## Pin the version

Use a tag that exists, like `@v0.2.3`. There's no `@v1` tag yet, so don't reference one.

## Inputs

| Input | Required | Default | What it is |
|-------|----------|---------|------------|
| `api_key` | Yes | | Your Open Cloud API key. |
| `command` | No | `run` | `run`, `publish`, `validate`, or `export`. |
| `config` | No | `rblxsync.yml` | Path to your config file. |
| `args` | No | `""` | Extra flags, like `--dry-run`. |
| `roblox_cookie` | No | `""` | Your `.ROBLOSECURITY` cookie. |

One catch with `args`: it gets split on spaces with no way to quote anything. Stick to plain flags like `--dry-run`. Anything with a space in it will break apart.

## Preview on pull requests, sync on main

A nice setup: pull requests show you what would change, and only merges to `main` actually change it.

```yaml
name: Roblox

on:
  pull_request:
  push:
    branches: [main]

jobs:
  preview:
    if: github.event_name == 'pull_request'
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dig1t/rblxsync@v0.2.3
        with:
          api_key: ${{ secrets.ROBLOX_API_KEY }}
          command: run
          args: --dry-run

  sync:
    if: github.event_name == 'push'
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dig1t/rblxsync@v0.2.3
        with:
          api_key: ${{ secrets.ROBLOX_API_KEY }}
          roblox_cookie: ${{ secrets.ROBLOX_COOKIE }}
          command: run
```

One thing to remember: when the action creates something new, it writes IDs into `rblxsync.yml` and `rblxsync-lock.yml` on the runner, and those changes vanish when the job ends. Run `rblxsync run` on your own machine first and commit the result, so CI has the IDs already.

Full input list: [API reference](/api#github-action).
