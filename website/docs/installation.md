---
sidebar_position: 2
title: Installation
---

# Installation

## Get the binary

Pick whichever of these three suits you.

### A tool manager (easiest)

If your project already uses Rokit, Aftman, or Foreman, it's one line:

```bash
rokit add dig1t/rblxsync@0.2.2
```

Or add it to `rokit.toml`, `aftman.toml`, or `foreman.toml` yourself and run the install:

```toml
[tools]
rblxsync = "dig1t/rblxsync@0.2.2"
```

This pins the version, so everyone on your team runs the same rblxsync.

### A direct download

Every version gets a [release](https://github.com/dig1t/rblxsync/releases) with a zip for Windows, Linux, Apple Silicon Macs, and Intel Macs. Download the one for your machine, unzip it, and put the `rblxsync` binary somewhere on your PATH.

| Your machine | The file you want |
| --- | --- |
| Mac, M1 or newer | `aarch64-apple-darwin.zip` |
| Mac, Intel | `x86_64-apple-darwin.zip` |
| Windows | `x86_64-pc-windows-msvc.zip` |
| Linux | `x86_64-unknown-linux-gnu.zip` |

### From source

Needs [Rust](https://rustup.rs).

```bash
git clone https://github.com/dig1t/rblxsync
cd rblxsync
cargo install --path .
```

### Check it worked

```bash
rblxsync --version
```

That prints the version it installed. `rblxsync --help` lists every command.

## Get an API key

An API key is a password that lets rblxsync talk to Roblox for you.

1. Go to [create.roblox.com/dashboard/credentials](https://create.roblox.com/dashboard/credentials)
2. Click **Create API Key**
3. Give it access to your experience, and turn on read + write for **Game Passes**, **Developer Products**, **Badges**, **Assets**, and **Places**
4. Under IP access, put `0.0.0.0/0` if you're not sure what your IP is
5. Save it and copy the key

## Put the key in a `.env` file

Make a file named `.env` in your project folder:

```bash
ROBLOX_API_KEY=paste_your_key_here
```

Then add `.env` to your `.gitignore`.

**Never** put this key on GitHub. Anyone who has it can change your game.

If you plan to change universe settings (name, description, devices, private server price), you need one more line. See [gotchas](/gotchas#universe-settings-need-a-cookie) for why.

```bash
ROBLOX_COOKIE=your_roblosecurity_cookie
```
