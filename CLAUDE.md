# CLAUDE.md — Nueva Implementation

## Project Overview
Nueva is a functional audio processing system with two parallel interfaces:
1. **Traditional DSP Controls**: Parameter-based effects (EQ, compression, reverb)
2. **AI Agent Interface**: Natural language commands invoking AI audio processing

Full spec: `NUEVA_IMPLEMENTATION.md`

---

## Ralph Loop Protocol

Every implementation task follows this cycle until complete:

```
┌─────────────────────────────────────────────────────────────┐
│  RALPH LOOP: Implement → Verify → Iterate → Checkpoint      │
└─────────────────────────────────────────────────────────────┘

1. IMPLEMENT
   - Write code for the current milestone
   - Follow the spec exactly — no creative interpretation
   - If spec is ambiguous, check edge cases section first

2. VERIFY
   - Run ALL relevant tests (unit + integration)
   - Check: Does it compile? Does it pass tests? Does it match spec?
   - Use verification commands (see below)

3. ITERATE
   - If verification fails: fix and re-verify (do NOT move on)
   - If verification passes: document what was done
   - Max 3 iterations per sub-task before escalating

4. CHECKPOINT
   - Commit with descriptive message: `[PHASE-X.Y] Brief description`
   - Update progress tracker in this file
   - If you learned something, ADD A RULE below
```

### Verification Commands
```bash
# Build check
cargo build --all-features 2>&1 | head -50

# Run tests for specific module
cargo test <module_name> -- --nocapture

# Audio verification (no listening required)
nueva-cli verify <file.wav>  # RMS, peak, crest factor, FFT

# Full test suite
cargo test --all

# Lint + format
cargo clippy && cargo fmt --check
```

### Termination Conditions
- **Success**: All phase milestones verified, tests pass, spec requirements met
- **Escalate**: 3+ failed iterations on same issue → ask user
- **Block**: Missing dependency/info not in spec → ask user

---

## Subagent Protocol

Append **"use subagents"** to requests for parallel compute. Subagents handle:

### When to Spawn Subagents
- Implementing multiple independent effects (each effect = 1 subagent)
- Writing tests while implementing features
- Analyzing large files while coding
- Any task that can be parallelized without shared state

### Subagent Rules
1. Each subagent gets ONE focused task
2. Subagent context stays clean — don't dump main context into it
3. Subagent reports back: `{status: "done"|"blocked", files_changed: [...], notes: "..."}`
4. Main agent integrates subagent work, resolves conflicts

### Example Subagent Delegation
```
Main task: "Implement DSP effects library"

Spawn subagents:
  - Subagent A: Implement EQ (parametric, shelf, filters)
  - Subagent B: Implement Dynamics (compressor, gate, limiter)  
  - Subagent C: Implement Time-based (delay, reverb)
  - Subagent D: Write tests for all effects

Main agent: Integrate, ensure chain ordering works, verify
```

---

## Worktree Coordination

This project uses parallel git worktrees. Each worktree runs independent Claude sessions.

### Worktree Assignments
| Worktree | Focus | Key Files |
|----------|-------|-----------|
| `wt-engine` | Audio Engine, Transport, Layers | `src/engine/`, `src/layers/` |
| `wt-dsp` | DSP Effects Library | `src/dsp/`, `src/effects/` |
| `wt-ai` | AI Agent, Neural Tools, Decision Logic | `src/agent/`, `src/neural/` |
| `wt-state` | State Management, Persistence, CLI | `src/state/`, `src/cli/` |
| `wt-test` | Testing, Integration, Verification | `tests/`, `benches/` |

### Merge Protocol
1. Each worktree commits to its own branch
2. Main agent (or user) merges when phase complete
3. Conflicts resolved in `main` worktree only

---

## Implementation Phases (from spec)

### Phase 1: Audio Engine Foundation [COMPLETE]
- [x] Layer 0: Immutable source storage (`src/layers/layer0.rs`)
- [x] Layer 1: AI state buffer (`src/layers/layer1.rs`)
- [x] Layer 2: DSP chain (`src/layers/layer2.rs`)
- [x] Transport state machine (`src/engine/transport.rs`)
- [x] Basic playback/export (`src/engine/io.rs`)

### Phase 2: DSP Effects Library [COMPLETE]
- [x] EQ (parametric, shelf, HP/LP filters)
- [x] Dynamics (compressor, limiter, gate)
- [x] Time-based (delay, reverb)
- [x] Utility (gain, saturation)
- [x] Effect chain ordering (`src/dsp/chain.rs`)

### Phase 3: AI/Neural Integration [COMPLETE]
- [x] Mock AI models (`src/neural/mock.rs`)
- [x] Model interface abstraction (`src/neural/model.rs`)
- [x] Neural tool routing (`src/neural/registry.rs`)
- [x] Context tracking for artifacts (`src/neural/context.rs`)
- [x] ACE-Step 1.5 integration (`src/neural/ace_step.rs`, `python/nueva/`)
- [x] Python bridge for AI models (`python/nueva/bridge.py`)

### Phase 4: Agent & Decision Logic [COMPLETE]
- [x] Prompt/intent parsing (`src/agent/intent.rs`)
- [x] Tool selection DSP/Neural/Both (`src/agent/decision.rs`)
- [x] Confidence scoring (`src/agent/decision.rs`)
- [x] Safety checks (`src/agent/safety.rs`)
- [x] Reference resolution (`src/agent/reference.rs`)
- [x] Explanation generation (`src/agent/explain.rs`)

### Phase 5: State & CLI [COMPLETE]
- [x] Project serialization JSON (`src/state/project.rs`)
- [x] Undo/redo stack (`src/state/undo.rs`)
- [x] Autosave (`src/state/autosave.rs`)
- [x] Crash recovery (`src/state/crash_recovery.rs`)
- [x] Schema migrations (`src/state/migration.rs`)
- [x] CLI commands (`src/cli/commands.rs`)
- [ ] Bake operation - needs integration
- [ ] Daemon mode (optional)

---

## Code Standards

### File Organization
```
nueva/
├── src/
│   ├── engine/       # Audio engine, transport, buffers
│   ├── layers/       # Layer 0, 1, 2 management
│   ├── dsp/          # DSP effect implementations
│   ├── agent/        # AI agent, decision logic
│   ├── neural/       # Neural model interfaces
│   ├── state/        # Persistence, undo/redo
│   └── cli/          # Command-line interface
├── tests/
│   ├── unit/
│   └── integration/
└── CLAUDE.md
```

### Naming Conventions
- Files: `snake_case.rs`
- Types: `PascalCase`
- Functions: `snake_case`
- Constants: `SCREAMING_SNAKE_CASE`
- Effect IDs: `kebab-case` (e.g., `parametric-eq-1`)

### Error Handling
- Use `Result<T, NuevaError>` everywhere
- Never panic in library code
- Always provide recovery path (see spec §9)

### Testing Requirements
- Every public function has at least one test
- Edge cases from spec §10 must have explicit tests
- Audio verification: RMS, peak, FFT — no manual listening

---

## Self-Updating Rules

**After ANY correction or learned behavior, add a rule here.**

### Learned Rules
<!-- Claude adds rules here after corrections -->

1. **[Rust Edition]** Cargo init may generate `edition = "2024"` which doesn't exist yet - always fix to `edition = "2021"`
2. **[Subagent Parallelism]** Spawning 8 parallel subagents for independent effect implementations works well - each effect has no shared state

---

## Current Session State

### Active Phase
<!-- Update this as you progress -->
Phase: All core phases COMPLETE
Worktree: wt-engine (synced with master)
Last checkpoint: All worktrees merged to master

### Blockers
<!-- List any blockers here -->
None

### Notes
<!-- Session-specific notes -->
- Full spec in `NUEVA_IMPLEMENTATION (3).md`
- **391 unit tests + 12 doc-tests passing**
- All core modules implemented across worktrees (wt-engine, wt-dsp, wt-ai, wt-state)
- ACE-Step 1.5 integrated: `src/neural/ace_step.rs` (Rust) + `python/nueva/` (Python bridge)
- ACE-Step installed at `C:\Users\dawha\ACE-Step-1.5` with models downloaded
- Remaining work: Bake operation, Daemon mode (optional)

---

## Quick Reference

### Spec Locations
- Layer model: §2
- Audio engine: §3
- DSP effects: §4
- AI agent: §5
- Decision logic: §6
- Error handling: §9
- Edge cases: §10
- Phases: §13

### Key Design Principles
1. Never destroy user work (non-destructive until explicit bake)
2. Fail gracefully (every error has recovery)
3. Predictable behavior (deterministic where possible)
4. Transparent AI (user can always see what AI did)
5. Escape hatches (user can override/undo/bypass)
6. Offline-first (core works without internet)

### Safety Checks (Always Run)
- Clipping: Peak must be < 0dBFS (warn if > -1dBFS)
- Phase: Stereo correlation must stay > 0.2
- Loudness: LUFS should not exceed -5 (streaming limit)
- Duration: Output must match input within 0.1s
