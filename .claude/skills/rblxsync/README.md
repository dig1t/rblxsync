# rblxsync Claude Code skill

A Claude Code skill that teaches Claude how to work with
[`rblxsync`](https://github.com/dig1t/rblxsync) — set it up, author/edit
`rblxsync.yml`, run safe syncs, and wire the generated `Config.luau` into Luau
game code.

## Install

Copy the `rblxsync/` skill folder into one of:

- **This project only:** `<your-project>/.claude/skills/rblxsync/`
- **All your projects (global):** `~/.claude/skills/rblxsync/`

```bash
# Global install from a checkout of the rblxsync repo:
mkdir -p ~/.claude/skills
cp -R .claude/skills/rblxsync ~/.claude/skills/rblxsync
```

The skill is then available automatically (Claude invokes it by relevance) or via
`/rblxsync` in Claude Code. No restart needed.

## What's inside

```
rblxsync/
├── SKILL.md                          # main workflow + golden rules
├── references/
│   ├── config-schema.md              # full rblxsync.yml field reference
│   ├── cli-and-ci.md                 # commands, env vars, Action, permissions
│   └── integration.md                # consuming Config.luau, lock file, idempotency
└── assets/
    ├── rblxsync.example.yml          # annotated config template
    └── github-workflow-sync.yml      # CI workflow template
```

## Verify

```bash
# from the skill-creator skill, if installed:
python ~/.claude/skills/skill-creator/scripts/quick_validate.py ~/.claude/skills/rblxsync
```
