---
sidebar_position: 5
title: Gotchas
---

# Things that will trip you up

## Badges are free 5 a day, then 100 Robux each

Roblox gives every game 5 free badges per day (GMT). Before it changes anything, rblxsync reads how many are left today and prints it. It creates badges for free while they last. If a run needs more, it stops before touching anything unless you pass `--allow-paid-badges`. Each badge goes out with the price rblxsync expects, so Roblox refuses it rather than charge a different amount.

Set `badge_payment_source` to `"user"` or `"group"` so Roblox knows which wallet a paid badge comes out of. Without it, badge creation fails.

## Universe settings need a cookie

Roblox has no Open Cloud endpoint for changing your game's name or description, so rblxsync signs in with your browser cookie instead. If you set *any* field under `universe` besides `id`, add this to `.env`:

```bash
ROBLOX_COOKIE=your_roblosecurity_cookie
```

To get it: log into roblox.com, press F12, go to Application → Cookies, and copy `.ROBLOSECURITY`.

Guard it like your password. Anyone with that string is logged in as you. Never commit it, never paste it anywhere public.

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

## Run rblxsync from the folder that holds `rblxsync-lock.yml`

The lock file is read from wherever your config lives but written to whatever folder you're standing in. Those being different will confuse it.

## `export` is not a config file

`rblxsync export` gives you a flat Lua table for reading and copying out of. It is not a `rblxsync.yml` and you can't feed it back into `rblxsync run`. If you want a real config from a live game, use [`rblxsync import`](/quick-start#already-have-a-game-with-stuff-in-it) instead.
