# Nueva Implementation Setup — Manual Commands

## 1. Initial Setup

```bash
# Navigate to your nueva project (or create it)
cd ~/projects/nueva  # adjust path as needed

# If starting fresh
mkdir nueva && cd nueva
git init
cargo init --name nueva

# Copy the files into place
cp /path/to/NUEVA_IMPLEMENTATION.md .
cp /path/to/CLAUDE.md .
cp /path/to/NUEVA_CLAUDE_CODE_PROMPT.md .

# Initial commit
git add -A
git commit -m "Initial setup with implementation spec"
```

---

## 2. Create Worktrees

```bash
# From your main nueva directory

# Create branches first
git branch wt-engine
git branch wt-dsp
git branch wt-ai
git branch wt-state
git branch wt-test

# Create worktrees (each gets its own directory)
git worktree add ../nueva-engine wt-engine
git worktree add ../nueva-dsp wt-dsp
git worktree add ../nueva-ai wt-ai
git worktree add ../nueva-state wt-state
git worktree add ../nueva-test wt-test

# Verify
git worktree list
```

You'll now have:
```
~/projects/
├── nueva/          # main (integration)
├── nueva-engine/   # Audio engine work
├── nueva-dsp/      # DSP effects work
├── nueva-ai/       # AI agent work
├── nueva-state/    # State/CLI work
└── nueva-test/     # Testing work
```

---

## 3. Shell Aliases (Optional but Recommended)

Add to your `~/.zshrc` or `~/.bashrc`:

```bash
# Quick navigation to nueva worktrees
alias za='cd ~/projects/nueva'
alias zb='cd ~/projects/nueva-engine'
alias zc='cd ~/projects/nueva-dsp'
alias zd='cd ~/projects/nueva-ai'
alias ze='cd ~/projects/nueva-state'
alias zf='cd ~/projects/nueva-test'

# Open Claude Code in specific worktree
alias cc-engine='cd ~/projects/nueva-engine && claude'
alias cc-dsp='cd ~/projects/nueva-dsp && claude'
alias cc-ai='cd ~/projects/nueva-ai && claude'
alias cc-state='cd ~/projects/nueva-state && claude'
alias cc-test='cd ~/projects/nueva-test && claude'
```

Then: `source ~/.zshrc`

---

## 4. Launch Claude Code Sessions

Open 3-5 terminal windows/tabs. In each:

```bash
# Terminal 1 — Audio Engine (highest priority)
cd ~/projects/nueva-engine
claude

# Terminal 2 — DSP Effects
cd ~/projects/nueva-dsp
claude

# Terminal 3 — AI Agent
cd ~/projects/nueva-ai
claude

# Terminal 4 — State/CLI
cd ~/projects/nueva-state
claude

# Terminal 5 — Testing (can start later)
cd ~/projects/nueva-test
claude
```

---

## 5. Start Each Session

Paste this into each Claude Code session (adjust worktree focus):

### For Engine Worktree:
```
Read NUEVA_IMPLEMENTATION.md and CLAUDE.md.

You are in the ENGINE worktree. Your focus:
- Audio Engine (§3)
- Layer Model (§2)  
- Transport state machine
- Basic playback/render

Start Phase 1 using Ralph Loop. Use subagents for parallel tasks.
Update CLAUDE.md after any correction.
```

### For DSP Worktree:
```
Read NUEVA_IMPLEMENTATION.md and CLAUDE.md.

You are in the DSP worktree. Your focus:
- DSP Effects Library (§4)
- All effect implementations
- Effect chain ordering

Start Phase 2 using Ralph Loop. Use subagents to parallelize effect implementations.
Update CLAUDE.md after any correction.
```

### For AI Worktree:
```
Read NUEVA_IMPLEMENTATION.md and CLAUDE.md.

You are in the AI worktree. Your focus:
- AI Agent Architecture (§5)
- Agent Decision Logic (§6)
- Neural tool interfaces
- Mock AI models first

Start Phase 3 using Ralph Loop. Use subagents for parallel tasks.
Update CLAUDE.md after any correction.
```

### For State Worktree:
```
Read NUEVA_IMPLEMENTATION.md and CLAUDE.md.

You are in the STATE worktree. Your focus:
- State Management (§8)
- Persistence (JSON serialization)
- CLI commands
- Undo/redo stack

Start Phase 5 using Ralph Loop. Use subagents for parallel tasks.
Update CLAUDE.md after any correction.
```

### For Test Worktree:
```
Read NUEVA_IMPLEMENTATION.md and CLAUDE.md.

You are in the TEST worktree. Your focus:
- Testing Strategy (§12)
- Unit tests for all modules
- Integration tests
- Audio verification (RMS, peak, FFT)

Build test harness first, then write tests as other worktrees implement features.
Update CLAUDE.md after any correction.
```

---

## 6. Merging Work

When a phase is complete in a worktree:

```bash
# From main nueva directory
cd ~/projects/nueva

# Merge engine work
git merge wt-engine -m "Merge Phase 1: Audio Engine"

# Merge DSP work  
git merge wt-dsp -m "Merge Phase 2: DSP Effects"

# etc.
```

**Handle conflicts in main only.** If conflicts arise, resolve them there, not in worktrees.

---

## 7. Opus 4.5 Permission Hook (Optional)

To auto-approve safe operations via Opus, create `.claude/hooks/permission.sh`:

```bash
#!/bin/bash
# Route permission requests to Opus 4.5 for security scan

REQUEST="$1"

# Call Opus to evaluate (simplified example)
# In practice, you'd use the Claude API here
VERDICT=$(echo "$REQUEST" | claude --model opus --query "Is this operation safe? Reply APPROVE or DENY only: ")

if [[ "$VERDICT" == *"APPROVE"* ]]; then
    echo "APPROVED"
    exit 0
else
    echo "DENIED: Requires manual approval"
    exit 1
fi
```

See: https://code.claude.com/docs/en/hooks#permissionrequest

---

## 8. Monitoring Progress

Keep a terminal open in main for monitoring:

```bash
cd ~/projects/nueva

# Watch all worktree activity
watch -n 5 'for dir in nueva-engine nueva-dsp nueva-ai nueva-state nueva-test; do echo "=== $dir ===" && cd ~/projects/$dir && git log --oneline -3 && cd -; done'

# Or just check status
git worktree list
```

---

## Quick Reference

| Task | Command |
|------|---------|
| List worktrees | `git worktree list` |
| Remove worktree | `git worktree remove ../nueva-engine` |
| Prune dead worktrees | `git worktree prune` |
| Check branch | `git branch` |
| Merge branch | `git merge wt-engine` |

---

## Troubleshooting

**"fatal: 'wt-engine' is already checked out"**
→ You can't checkout the same branch in two places. Use worktrees correctly.

**Worktree out of sync**
```bash
cd ~/projects/nueva-engine
git pull origin main  # or rebase
```

**Need to start over on a worktree**
```bash
git worktree remove ../nueva-engine
git branch -D wt-engine
git branch wt-engine
git worktree add ../nueva-engine wt-engine
```
