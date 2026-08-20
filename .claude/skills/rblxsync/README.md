# rblxsync Claude Code skill

This teaches Claude how to use [`rblxsync`](https://github.com/dig1t/rblxsync),
the tool that keeps your Roblox game passes, developer products, and badges in a
text file and syncs them to Roblox with one command.

With the skill installed, you can ask Claude things like "add a 200 Robux VIP
pass" or "why is rblxsync asking me for a cookie" and it already knows the rules,
the config format, and which mistakes cost you Robux.

## Install

Copy the `rblxsync/` folder to one of these spots:

- Just this project: `<your-project>/.claude/skills/rblxsync/`
- Every project you work on: `~/.claude/skills/rblxsync/`

From a checkout of the rblxsync repo, the second one looks like this:

```bash
mkdir -p ~/.claude/skills
cp -R .claude/skills/rblxsync ~/.claude/skills/rblxsync
```

That's it, no restart. Claude picks the skill up on its own when your question is
about rblxsync, or you can call it directly by typing `/rblxsync`.

## What's in the box

```
rblxsync/
├── SKILL.md                          # the main guide: workflow + golden rules
├── references/
│   ├── config-schema.md              # every rblxsync.yml field, explained
│   ├── cli-and-ci.md                 # commands, env vars, GitHub Action, permissions
│   └── integration.md                # using Config.luau, the lock file, safe re-runs
└── assets/
    ├── rblxsync.example.yml          # a config to copy and trim
    └── github-workflow-sync.yml      # a CI workflow to copy
```

Claude reads `SKILL.md` first and only opens a reference file when it needs the
detail, so the extra files cost you nothing until they're useful.

## Check it installed correctly

Optional. This only works if you also have the skill-creator skill:

```bash
python ~/.claude/skills/skill-creator/scripts/quick_validate.py ~/.claude/skills/rblxsync
```
