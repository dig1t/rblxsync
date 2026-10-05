---
sidebar_position: 5
title: Gotchas
---

# Things that will trip you up

## Badges are free 5 a day, then 100 Robux each

Roblox gives every game 5 free badges per day (GMT). Before it changes anything, rblxsync reads how many are left today and prints it. It creates badges for free while they last. If a run needs more, it stops before touching anything unless you pass `--allow-paid-badges`. Each badge goes out with the price rblxsync expects, so Roblox refuses it rather than charge a different amount.

Set `badge_payment_source` to `"user"` or `"group"` so Roblox knows which wallet a paid badge comes out of. Without it, badge creation fails.

## Set the name, description, and server size in one spot

`universe:` sets the root place's name, description, and (with `max_players`) server size. If the root place is also listed under `places:` with the same field, a run stops. Set each one under `universe:` or on the place, not both.

Earlier versions only wrote `max_players` into `Config.luau`. Now it changes the root place's server size, so check it matches your real one before you upgrade.

## Changing the private server price cancels subscriptions

Roblox cancels every private server subscription when the price changes, and rate-limits price changes. rblxsync reads your current settings first and only sends a price that differs, so an unchanged price is never re-sent.

## Set the genre yourself

Roblox only lets you change a genre once every 3 months, so rblxsync doesn't touch it. Set it in the Creator Hub. A `genre:` left in `universe:` is ignored with a warning.

## A place open in Team Create can't be changed

Roblox refuses place updates during a Team Create session. rblxsync only sends a place's settings when they differ from what Roblox has, so this only comes up after you change one. Close the session and run again.

## `is_active` on developer products does nothing

rblxsync reads it and ignores it. Game pass `is_for_sale` does work, and a new pass with a price goes on sale unless you set `is_for_sale: false`.

## rblxsync only deletes thumbnails it uploaded

Delete the others in the Creator Hub if you don't want them. Leaving out the `thumbnails` section entirely leaves your thumbnails alone.

## The images at the top of your game's page aren't synced

Roblox only lets an API key change translated copies of them (one set per language), never the set everyone sees, so upload those in the Creator Hub. A `thumbnails.game_page` in your config fails `validate`.

## Home page thumbnails use an experimental API

Roblox marks the thumbnail personalization API experimental. If Roblox changes it, `home_page` can break before the rest of rblxsync does.

## Run rblxsync from your project's root

`assets_dir`, `output_path`, and each place's `file_path` are relative to the folder you run it from. The lock file always sits next to the config.

## `export` is not a config file

`rblxsync export` gives you a flat Lua table for reading and copying out of. It is not a `rblxsync.yml` and you can't feed it back into `rblxsync run`. If you want a real config from a live game, use [`rblxsync import`](/quick-start#already-have-a-game-with-stuff-in-it) instead.
