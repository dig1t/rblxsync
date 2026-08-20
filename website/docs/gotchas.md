---
sidebar_position: 5
title: Gotchas
---

# Things that will trip you up

## Badges cost 100 Robux each

Every single one, charged the moment rblxsync creates it. Set `badge_payment_source` to `"user"` or `"group"` so Roblox knows which wallet to pull from. Without it, badge creation fails.

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

rblxsync reads it and ignores it. Game pass `is_for_sale` does work.

## `--config` goes before the command

```bash
rblxsync --config production.yml run   # works
rblxsync run --config production.yml   # error
```

## Run rblxsync from the folder that holds `rblxsync-lock.yml`

The lock file is read from wherever your config lives but written to whatever folder you're standing in. Those being different will confuse it.

## `export` is not a config file

`rblxsync export` gives you a flat Lua table for reading and copying out of. It is not a `rblxsync.yml` and you can't feed it back into `rblxsync run`. If you want a real config from a live game, use [`rblxsync import`](/quick-start#already-have-a-game-with-stuff-in-it) instead.
