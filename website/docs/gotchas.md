---
sidebar_position: 5
title: Gotchas
---

# Things that will trip you up

## Badges are free 5 a day, then 100 Robux each

Roblox gives every game 5 free badges per day (GMT). Before it changes anything, rblxsync reads how many are left today and prints it. It creates badges for free while they last. If a run needs more, it stops before touching anything unless you pass `--allow-paid-badges`. Each badge goes out with the price rblxsync expects, so Roblox refuses it rather than charge a different amount.

Set `badge_payment_source` to `"user"` or `"group"` so Roblox knows which wallet a paid badge comes out of. Without it, badge creation fails.

## Universe settings need a cookie

rblxsync changes everything under `universe` through a Roblox website endpoint that signs in with your browser cookie. If you set *any* field under `universe` besides `id`, add this to `.env`:

```bash
ROBLOX_COOKIE=your_roblosecurity_cookie
```

To get it: log into roblox.com, press F12, go to Application → Cookies, and copy `.ROBLOSECURITY`.

Guard it like your password. Anyone with that string is logged in as you. Never commit it, never paste it anywhere public.

## You can skip the cookie for the name and description

Your game's name and description are its starting place's. Leave them out of `universe:` and set `name` and `description` on the starting place under `places:` instead, and the API key is enough. Set them in one spot only: a run stops if both set the same field.

## A place open in Team Create can't be changed

Roblox refuses place updates during a Team Create session. rblxsync only sends a place's settings when they differ from what Roblox has, so this only comes up after you change one. Close the session and run again.

## The cookie rule is greedy

It kicks in even for `genre` and `max_players`, which rblxsync only tracks locally and never sends anywhere. Setting just `genre` will still stop and ask for a cookie.

## `genre` and `max_players` never reach Roblox

They get saved to the lock file and your `Config.luau`, and that's it. Change them in Studio or the Creator Hub.

## `is_active` on developer products does nothing

rblxsync reads it and ignores it. Game pass `is_for_sale` does work, and a new pass with a price goes on sale unless you set `is_for_sale: false`.

## rblxsync only deletes thumbnails it uploaded

Images already on your game page stay after yours, still count toward the 10, and every run lists them in a warning. Delete them in the Creator Hub if you don't want them. Leaving out the `thumbnails` section entirely leaves your thumbnails alone.

## Home page thumbnails use an experimental API

Roblox marks the thumbnail personalization API experimental. If Roblox changes it, `home_page` can break before the rest of rblxsync does.

## Run rblxsync from your project's root

`assets_dir`, `output_path`, and each place's `file_path` are relative to the folder you run it from. The lock file always sits next to the config.

## `export` is not a config file

`rblxsync export` gives you a flat Lua table for reading and copying out of. It is not a `rblxsync.yml` and you can't feed it back into `rblxsync run`. If you want a real config from a live game, use [`rblxsync import`](/quick-start#already-have-a-game-with-stuff-in-it) instead.
