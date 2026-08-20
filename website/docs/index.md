---
slug: /
sidebar_position: 1
title: Introduction
---

# rblxsync

Your game passes, developer products, and badges live in a text file. You run one command. Roblox updates to match.

No more clicking through the Creator Hub every time you change a price. No more copying IDs into your code by hand.

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

## What it can do

- Create and update game passes, developer products, and badges from one file
- Upload icons, and skip the upload when the image hasn't changed
- Publish `.rbxl` place files
- Write every ID into a typed Luau module your game code can `require`
- Pull an existing game's setup down into a config, so you don't have to retype it
- Run the whole thing from GitHub Actions

## Where to go next

- [Installation](/installation) to get it on your machine
- [Quick start](/quick-start) to sync your first game pass
- [Gotchas](/gotchas) for the stuff that catches people out
- [API reference](/api) for every field and flag
