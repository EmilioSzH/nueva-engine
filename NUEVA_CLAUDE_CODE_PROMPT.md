# Nueva Implementation — Claude Code Prompt

## Your Mission

You are implementing **Nueva**, a complete audio processing system. The full specification is in `NUEVA_IMPLEMENTATION.md`. You have `CLAUDE.md` which contains your working memory, learned rules, and protocols.

---

## Execution Protocol: Ralph Loop

For EVERY task, follow this cycle until complete:

```
IMPLEMENT → VERIFY → ITERATE → CHECKPOINT
```

1. **IMPLEMENT**: Write code matching the spec exactly
2. **VERIFY**: Run tests, check compilation, validate against spec
3. **ITERATE**: Fix failures (max 3 attempts, then escalate to user)
4. **CHECKPOINT**: Commit, update CLAUDE.md progress tracker

**Never skip verification. Never move on with failing tests.**

---

## Parallel Execution Strategy

You're working in a multi-worktree setup. Focus on YOUR assigned worktree:

| Worktree | Your Focus |
|----------|------------|
| `wt-engine` | Audio Engine, Transport, Layer model |
| `wt-dsp` | DSP Effects (EQ, dynamics, time-based) |
| `wt-ai` | AI Agent, Neural tools, Decision logic |
| `wt-state` | State management, Persistence, CLI |
| `wt-test` | Testing harness, Integration tests |

**Stay in your lane.** Don't modify files outside your focus area unless absolutely necessary.

---

## Subagent Usage

For complex tasks, spawn subagents to parallelize work:

```
// Add "use subagents" to any request for parallel compute

Example:
"Implement all DSP effects. Use subagents."

You would spawn:
- Subagent 1: EQ effects
- Subagent 2: Dynamics effects  
- Subagent 3: Time-based effects
- Subagent 4: Tests for all
```

Each subagent gets ONE focused task. You integrate their output.

---

## Self-Improvement Protocol

After ANY mistake or correction:

1. Identify what went wrong
2. Formulate a rule that prevents it
3. Add the rule to `CLAUDE.md` under "Learned Rules"
4. Say: "Updated CLAUDE.md to prevent this in future"

Example:
```
User: "You forgot to check for clipping before applying gain"
You: *fix the code*
You: *add to CLAUDE.md*: "Rule: Always run clipping check before any gain operation"
You: "Updated CLAUDE.md to prevent this in future."
```

---

## Implementation Order (from spec Appendix C)

1. **Phase 1: Audio Engine** [CRITICAL]
   - This is the foundation. If `--chain config.json` doesn't work, nothing works.
   - Layer 0, 1, 2 management
   - Transport state machine
   - Basic playback/render

2. **Phase 3.3: Mock AI**
   - Implement BEFORE real PyTorch models
   - Pipeline logic > inference quality for v1
   - Stub out neural tools with pass-through

3. **Phase 2: DSP Effects**
   - EQ, Compressor, Limiter, Gate
   - Delay, Reverb
   - Effect chain ordering

4. **Phase 4-5: Agent + State**
   - Decision logic
   - Persistence
   - CLI commands

---

## Verification Requirements

Before marking ANY milestone complete:

```bash
# Must pass
cargo build --all-features
cargo test --all
cargo clippy -- -D warnings

# For audio code specifically
cargo test audio_ -- --nocapture  # Visual inspection of test output
```

**Audio tests should verify programmatically** (RMS, peak, FFT) — no manual listening required.

---

## When You're Stuck

1. **Spec unclear?** → Check edge cases section (§10) first
2. **3+ failed iterations?** → Stop and ask user
3. **Missing dependency?** → Ask user before adding
4. **Conflict with another worktree?** → Note it, continue your work, user will merge

---

## Start Command

Begin with:
```
1. Read NUEVA_IMPLEMENTATION.md fully
2. Read CLAUDE.md for protocols
3. Report which phase you're starting
4. Begin Ralph Loop on first milestone
```

---

## Response Format

For each work session, structure your responses as:

```
## Current Task
[What you're implementing]

## Implementation
[Code or actions taken]

## Verification
[Test results, build status]

## Status
✅ PASS — proceeding to next milestone
❌ FAIL — iterating (attempt X/3)
⚠️ BLOCKED — [reason], asking user

## Next
[What's next in the phase]
```

---

## Critical Reminders

- **Never destroy user work** — all operations non-destructive until explicit bake
- **Always check clipping** — peak < 0dBFS, warn if > -1dBFS
- **Preserve Layer 2** by default when regenerating Layer 1
- **Test edge cases**: silent audio, clipped audio, mono→stereo, very long files
- **Update CLAUDE.md** after every correction you receive

---

Now read the spec and CLAUDE.md, then begin Phase 1 of implementation using Ralph Loop.
