# Nueva Implementation Specification
## Complete Technical Specification for Claude Code

**Version**: 1.0  
**Last Updated**: 2025-02-04  
**Target**: Claude Code autonomous implementation (one-shot)

---

## Table of Contents

1. [Overview & Philosophy](#overview)
2. [Core Architecture: The Layer Model](#layer-model)
3. [Audio Engine Specification](#audio-engine)
4. [DSP Effects Library](#dsp-effects)
5. [AI Agent Architecture](#ai-agent)
6. [Agent Decision Logic](#agent-decisions)
7. [Conversation & Context Management](#conversation)
8. [State Management & Persistence](#state-management)
9. [Error Handling Matrix](#error-handling)
10. [Edge Cases & Special Scenarios](#edge-cases)
11. [Multi-Track Architecture](#multi-track)
12. [Testing Strategy](#testing)
13. [Implementation Phases](#phases)
14. [File Formats & Protocols](#formats)
15. [Security Considerations](#security)
16. [Performance Requirements](#performance)

---

## 1. Overview & Philosophy <a name="overview"></a>

Nueva is a functional audio processing system that provides **two parallel interfaces** for manipulating audio:
1. **Traditional DSP Controls**: Parameter-based effects (EQ, compression, reverb, etc.)
2. **AI Agent Interface**: Natural language commands that invoke AI audio processing

The user can seamlessly switch between or combine both approaches. This is NOT a traditional plugin—it's a complete audio processing pipeline where AI transformations and manual DSP coexist as separate layers.

### Design Principles

1. **Never Destroy User Work**: All operations are non-destructive until explicit export/bake
2. **Fail Gracefully**: Every error has a recovery path; user never loses progress
3. **Predictable Behavior**: Same input + same command = same output (deterministic where possible)
4. **Transparent AI**: User can always see/understand what the AI did
5. **Escape Hatches**: User can always override, undo, or bypass AI decisions
6. **Offline-First**: Core functionality works without internet; cloud features are optional enhancements

### What Nueva Is NOT

- NOT a real-time DAW with timeline/arrangement (that's future scope)
- NOT a plugin host (no VST/AU in v1)
- NOT a MIDI sequencer
- NOT a streaming/live performance tool

---

## 2. Core Architecture: The Layer Model <a name="layer-model"></a>

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Nueva Processing Engine                              │
├─────────────────────────────────────────────────────────────────────────────┤
│  LAYER 0: Source Audio (Immutable Reference)                                │
│  ├── Original uploaded/recorded audio                                       │
│  ├── NEVER modified after initial creation                                  │
│  ├── Stored as: WAV file (converted to 48kHz/24-bit internally)            │
│  └── Can only change via explicit "bake" operation                          │
├─────────────────────────────────────────────────────────────────────────────┤
│  LAYER 1: AI State                                                          │
│  ├── Output of AI/Neural transformations                                    │
│  ├── Initially: exact copy of Layer 0                                       │
│  ├── After AI processing: rendered result                                   │
│  ├── Stored as: WAV file + metadata JSON                                    │
│  └── Regenerated each time AI is invoked                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│  LAYER 2: DSP Chain (Real-time Adjustable)                                  │
│  ├── Traditional effects applied ON TOP of Layer 1                          │
│  ├── Stored as: Parameter state JSON (no rendered audio)                    │
│  ├── Processed in real-time during playback/export                          │
│  └── Non-destructive, infinitely adjustable                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│  OUTPUT: Final Rendered Audio                                               │
│  └── Computed as: Layer1_audio → DSP_Chain → Output                         │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 2.1 Layer Operations Matrix

| User Action | Layer 0 | Layer 1 | Layer 2 | Notes |
|-------------|---------|---------|---------|-------|
| Import audio | Created | Copied from L0 | Empty chain | Initial project state |
| Tweak DSP parameter | Unchanged | Unchanged | Updated | Instant, no re-render |
| Add DSP effect | Unchanged | Unchanged | Effect added | Order matters |
| Remove DSP effect | Unchanged | Unchanged | Effect removed | Reversible |
| AI prompt (DSP tool) | Unchanged | Unchanged | Modified by agent | Agent manipulates L2 |
| AI prompt (Neural tool) | Unchanged | **Regenerated** | See below | L1 replaced |
| AI prompt (Both tools) | Unchanged | **Regenerated** | Modified | Neural first, then DSP |
| "Undo" | Unchanged | May revert | May revert | Depends on what's being undone |
| "Bake" | **Replaced** | Reset to new L0 | **Cleared** | Destructive flatten |
| "Reset AI" | Unchanged | Reset to L0 copy | Unchanged | Clears neural processing |
| "Reset DSP" | Unchanged | Unchanged | **Cleared** | Clears effect chain |
| "Reset All" | Unchanged | Reset to L0 copy | **Cleared** | Back to import state |
| Export | Unchanged | Unchanged | Unchanged | Renders L1→L2→file |

### 2.2 Layer 2 Preservation on AI Re-invoke

**Critical Decision Point**: When user invokes Neural tool (which regenerates Layer 1), what happens to Layer 2?

**Default Behavior**: PRESERVE Layer 2 (user's manual tweaks are kept)

**Rationale**: User effort should not be discarded without explicit consent

**Edge Cases**:

| Scenario | Behavior | Rationale |
|----------|----------|-----------|
| User says "make it warmer" (neural) | L1 regenerated, L2 preserved | Don't lose their EQ tweaks |
| User says "start fresh with AI" | L1 regenerated, L2 cleared | Explicit reset intent |
| User says "redo AI but keep my EQ" | L1 regenerated, L2 preserved | Explicit preserve intent |
| User says "redo everything" | L1 regenerated, L2 cleared | Explicit full reset |
| AI processing fails | L1 unchanged, L2 unchanged | Fail-safe |
| AI processing produces silence | Prompt user, don't auto-apply | Safety check |
| AI processing is very different | Apply but warn user | Transparency |

**Implementation**:
```python
class LayerPreservationPolicy(Enum):
    PRESERVE_L2 = "preserve"      # Default: keep DSP chain
    RESET_L2 = "reset"            # Clear DSP chain
    ASK_USER = "ask"              # Prompt for decision
    SMART = "smart"               # Agent decides based on context

def determine_l2_policy(prompt: str, context: dict) -> LayerPreservationPolicy:
    """
    Analyze prompt to determine Layer 2 handling.
    """
    reset_signals = ["start fresh", "from scratch", "redo everything", "reset"]
    preserve_signals = ["keep my", "preserve", "but maintain", "don't touch"]
    
    prompt_lower = prompt.lower()
    
    if any(sig in prompt_lower for sig in reset_signals):
        return LayerPreservationPolicy.RESET_L2
    if any(sig in prompt_lower for sig in preserve_signals):
        return LayerPreservationPolicy.PRESERVE_L2
    
    # Default: preserve user work
    return LayerPreservationPolicy.PRESERVE_L2
```

### 2.3 The Bake Operation (Detailed)

"Bake" flattens all layers into a new source. This is the ONLY destructive operation.

**Pre-Bake Checklist** (system must verify):
1. ✓ Layer 1 is not silence
2. ✓ Final output is not clipping (peak < 0dBFS, warn if > -1dBFS)
3. ✓ Final output duration matches source (within 0.1s tolerance)
4. ✓ User has been warned this is destructive

**Bake Process**:
```
1. Render: L1 audio → L2 DSP chain → temp_baked.wav
2. Validate: Check temp file is valid audio
3. Backup: Move current L0 to backups/layer0_pre_bake_{timestamp}.wav
4. Replace: Move temp_baked.wav → layer0_source.wav
5. Reset L1: Copy new L0 → layer1_ai.wav
6. Clear L1 meta: Reset layer1_meta.json to unprocessed state
7. Clear L2: Empty the DSP effect chain
8. Update history: Log bake operation with timestamp
```

**Bake Failure Recovery**:
```
If step 1-2 fails: Abort, no changes made
If step 3 fails: Abort, warn about disk space
If step 4-7 fails: Restore from backup, report error
```

---

## 3. Audio Engine Specification <a name="audio-engine"></a>

### 3.0 Transport State Machine (From DAWN)

Nueva inherits DAWN's proven transport state management. When the user activates the AI agent (via button, hotkey, or voice), the system must pause any active playback/recording to ensure coherent processing.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                       TRANSPORT STATE MACHINE                                │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│    ┌──────────┐         play          ┌──────────┐                          │
│    │  PAUSED  │ ───────────────────►  │ PLAYING  │                          │
│    │          │ ◄───────────────────  │          │                          │
│    └────┬─────┘     pause/stop        └────┬─────┘                          │
│         │                                   │                                │
│         │ record                           │ (auto-pause on agent invoke)   │
│         ▼                                   │                                │
│    ┌──────────┐                            │                                │
│    │RECORDING │ ◄──────────────────────────┘                                │
│    │          │        record while playing                                  │
│    └────┬─────┘                                                              │
│         │                                                                    │
│         │ stop/keep/discard                                                  │
│         ▼                                                                    │
│    ┌──────────┐                                                              │
│    │  PAUSED  │                                                              │
│    └──────────┘                                                              │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Agent Invocation Protocol** (Critical for coherent interaction):

```cpp
enum class TransportState { PAUSED, PLAYING, RECORDING };

class TransportManager {
    TransportState state = TransportState::PAUSED;
    double saved_playhead_position = 0.0;
    
    void onAgentInvoked() {
        // AUTO-PAUSE: Always pause before processing agent command
        // This ensures audio state is stable during AI processing
        
        if (state == TransportState::RECORDING) {
            // Save current position for potential resume
            saved_playhead_position = getCurrentPlayheadPosition();
            // Stop recording, keep buffer for potential "keep that"
            stopRecording(/* discard = */ false);
            state = TransportState::PAUSED;
            log("[AUTO-PAUSE] Stopped recording, saved position");
        }
        else if (state == TransportState::PLAYING) {
            saved_playhead_position = getCurrentPlayheadPosition();
            pause();
            state = TransportState::PAUSED;
            log("[AUTO-PAUSE] Paused playback, saved position");
        }
        // If already PAUSED, no action needed
        
        // Now safe to process agent command
    }
    
    void onAgentComplete(AgentResult result) {
        // Agent finished - decide what to do with transport
        
        if (result.should_resume_playback) {
            seekTo(saved_playhead_position);
            play();
            state = TransportState::PLAYING;
        }
        // Otherwise stay paused so user can hear the change
    }
};
```

**Agent Response Latency Expectation**:
Even for simple DSP commands, there will be a ~200-500ms delay from:
1. Agent receiving prompt
2. LLM processing (if using reasoning)
3. DSP parameter application
4. Audio engine update

**UI Feedback During Processing**:
```
[User clicks "Agent" button or presses hotkey]
   │
   ▼
[Transport auto-pauses]
[UI shows: "Processing..." spinner]
[Agent works...]
[UI shows: "Applied: EQ boost at 3kHz" for 3 seconds]
[UI returns to normal]
```

**Clipping Prevention Dialog** (Do No Harm Rule):
If the agent detects that its changes will cause clipping:
```
┌────────────────────────────────────────────────────┐
│  ⚠️ I added a limiter to prevent clipping         │
│                                                    │
│  Your requested changes would have pushed         │
│  peaks above 0dB. I've inserted a limiter at     │
│  -1dB ceiling to protect your audio.             │
│                                                    │
│                          [OK]  [Remove Limiter]   │
└────────────────────────────────────────────────────┘
        ↓ (auto-dismiss after 3 seconds)
```

---

### 3.1 Supported Formats

| Format | Import | Export | Notes |
|--------|--------|--------|-------|
| WAV | ✓ | ✓ | Primary format, all bit depths |
| AIFF | ✓ | ✓ | Mac compatibility |
| FLAC | ✓ | ✓ | Lossless compressed |
| MP3 | ✓ | ✓ | Lossy, via LAME |
| OGG | ✓ | ✓ | Lossy, open format |
| M4A/AAC | ✓ | ✗ | Import only (licensing) |

### 3.2 Internal Processing Format

**All audio is converted to internal format on import:**
- Sample Rate: 48,000 Hz
- Bit Depth: 32-bit float (for headroom)
- Channels: Preserve original (mono/stereo)

**Rationale**: 
- 48kHz is professional standard, compatible with video
- 32-bit float allows >0dBFS without clipping during processing
- Final export converts to user-specified format

### 3.3 Sample Rate Conversion

| Input SR | Action | Quality Setting |
|----------|--------|-----------------|
| 44.1 kHz | Upsample to 48k | High quality (sinc interpolation) |
| 48 kHz | Pass through | N/A |
| 88.2 kHz | Downsample to 48k | High quality |
| 96 kHz | Downsample to 48k | High quality |
| Other | Resample to 48k | High quality |

**Implementation**: Use JUCE's `LagrangeInterpolator` or `CatmullRomInterpolator`

### 3.4 Channel Handling

| Input | Internal | Behavior |
|-------|----------|----------|
| Mono | Mono | Process as mono |
| Stereo | Stereo | Process as stereo |
| Mono → Stereo FX | Stereo | If effect outputs stereo (reverb), convert to stereo |
| Multi-channel (5.1) | **Reject** | v1 does not support surround |

### 3.5 Duration Limits

| Duration | Behavior |
|----------|----------|
| < 0.1 seconds | Reject with error: "Audio too short" |
| 0.1s - 30 minutes | Normal processing |
| 30 min - 2 hours | Warn user about memory/time, proceed if confirmed |
| > 2 hours | Reject with error: "Audio too long for single-file processing" |

### 3.6 Audio Validation

On every audio load, verify:

```cpp
struct AudioValidation {
    bool valid;
    std::string error;
    
    // Checks performed:
    bool has_samples;        // File contains actual audio data
    bool reasonable_length;  // Within duration limits
    bool not_corrupt;        // Can decode without errors
    bool not_silent;         // RMS > -80dBFS
    bool not_dc_offset;      // Mean sample value < 0.01
    bool not_clipped;        // < 1% of samples at ±1.0
};

AudioValidation validate_audio(const AudioBuffer& buffer) {
    AudioValidation v;
    v.valid = true;
    
    if (buffer.getNumSamples() == 0) {
        v.valid = false;
        v.error = "Audio file contains no samples";
        v.has_samples = false;
        return v;
    }
    
    float rms = calculate_rms(buffer);
    if (rms < db_to_linear(-80.0f)) {
        v.not_silent = false;
        // Don't fail, but warn
        log_warning("Audio appears to be silent or near-silent");
    }
    
    float mean = calculate_mean(buffer);
    if (std::abs(mean) > 0.01f) {
        v.not_dc_offset = false;
        log_warning("Audio has DC offset, consider removing");
    }
    
    float clip_ratio = calculate_clip_ratio(buffer);
    if (clip_ratio > 0.01f) {
        v.not_clipped = false;
        log_warning("Audio appears to be clipped (%.1f%% samples at max)", clip_ratio * 100);
    }
    
    return v;
}
```

---

## 4. DSP Effects Library <a name="dsp-effects"></a>

### 4.1 Effect Base Class

```cpp
class Effect {
public:
    virtual ~Effect() = default;
    
    // Core processing
    virtual void process(AudioBuffer& buffer) = 0;
    virtual void prepare(double sampleRate, int samplesPerBlock) = 0;
    virtual void reset() = 0;
    
    // Serialization
    virtual json toJson() const = 0;
    virtual void fromJson(const json& j) = 0;
    
    // Metadata
    virtual std::string getType() const = 0;
    virtual std::string getDisplayName() const = 0;
    
    // State
    bool enabled = true;
    std::string id;  // Unique identifier for this instance
    
    // Parameter change notification
    std::function<void()> onParameterChanged;
};
```

### 4.2 Complete Effect Specifications

#### 4.2.1 Gain

```cpp
class GainEffect : public Effect {
    float gainDb = 0.0f;  // Range: -96 to +24 dB
    
    // Implementation: Simple multiplication
    // gain_linear = pow(10, gainDb / 20)
    // sample *= gain_linear
};
```

**Agent Usage Examples**:
- "make it louder" → +3 to +6 dB
- "turn it down" → -3 to -6 dB  
- "boost it significantly" → +9 to +12 dB
- "make it barely audible" → -24 to -48 dB

#### 4.2.2 Parametric EQ

```cpp
class ParametricEQ : public Effect {
    struct Band {
        float frequency;  // 20 - 20000 Hz
        float gainDb;     // -24 to +24 dB
        float q;          // 0.1 to 10.0
        FilterType type;  // peak, lowshelf, highshelf, lowpass, highpass
        bool enabled = true;
    };
    
    std::vector<Band> bands;  // Max 8 bands
    
    // Implementation: Cascaded biquad filters (JUCE IIR)
};

enum class FilterType {
    PEAK,       // Bell curve boost/cut
    LOW_SHELF,  // Boost/cut below frequency
    HIGH_SHELF, // Boost/cut above frequency
    LOW_PASS,   // Remove above frequency
    HIGH_PASS   // Remove below frequency
};
```

**Agent Frequency Vocabulary**:
```
"sub bass" / "rumble"      → 20-60 Hz
"bass"                      → 60-250 Hz
"low mids" / "muddiness"   → 250-500 Hz
"mids"                      → 500-2000 Hz
"upper mids" / "presence"  → 2000-4000 Hz
"highs" / "treble"         → 4000-8000 Hz
"air" / "brilliance"       → 8000-20000 Hz
```

**Agent Intent → EQ Mapping**:
```python
EQ_INTENT_MAP = {
    "warmer": [
        {"type": "highshelf", "freq": 8000, "gain": -2.0},
        {"type": "lowshelf", "freq": 200, "gain": +1.5}
    ],
    "brighter": [
        {"type": "highshelf", "freq": 8000, "gain": +3.0}
    ],
    "less muddy": [
        {"type": "peak", "freq": 300, "gain": -4.0, "q": 1.0}
    ],
    "more presence": [
        {"type": "peak", "freq": 3000, "gain": +3.0, "q": 1.5}
    ],
    "more air": [
        {"type": "highshelf", "freq": 12000, "gain": +2.0}
    ],
    "remove rumble": [
        {"type": "highpass", "freq": 80, "q": 0.7}
    ],
    "telephone effect": [
        {"type": "highpass", "freq": 300, "q": 0.7},
        {"type": "lowpass", "freq": 3000, "q": 0.7}
    ]
}
```

#### 4.2.3 Compressor

```cpp
class Compressor : public Effect {
    float thresholdDb = -18.0f;  // -60 to 0 dB
    float ratio = 4.0f;           // 1:1 to 20:1
    float attackMs = 10.0f;       // 0.1 to 100 ms
    float releaseMs = 100.0f;     // 10 to 1000 ms
    float kneeDb = 0.0f;          // 0 (hard) to 12 dB (soft)
    float makeupGainDb = 0.0f;    // 0 to 24 dB
    bool autoMakeup = false;
    
    // Implementation: Envelope follower + gain computer + smoothing
};
```

**Agent Compression Vocabulary**:
```python
COMPRESSION_PRESETS = {
    "gentle glue": {"threshold": -24, "ratio": 2, "attack": 30, "release": 200},
    "punchy": {"threshold": -18, "ratio": 4, "attack": 5, "release": 80},
    "aggressive": {"threshold": -12, "ratio": 8, "attack": 1, "release": 50},
    "limiting": {"threshold": -6, "ratio": 20, "attack": 0.1, "release": 100},
    "transparent": {"threshold": -30, "ratio": 1.5, "attack": 50, "release": 300},
    "vocal control": {"threshold": -20, "ratio": 3, "attack": 15, "release": 150},
    "drum punch": {"threshold": -15, "ratio": 6, "attack": 3, "release": 60},
}
```

#### 4.2.4 Reverb

```cpp
class Reverb : public Effect {
    float roomSize = 0.5f;     // 0 (tiny) to 1 (huge hall)
    float damping = 0.5f;      // 0 (bright) to 1 (dark)
    float wetLevel = 0.3f;     // 0 to 1
    float dryLevel = 1.0f;     // 0 to 1
    float width = 1.0f;        // 0 (mono) to 1 (full stereo)
    float preDelayMs = 0.0f;   // 0 to 100 ms
    
    // Implementation: JUCE Reverb or Freeverb algorithm
};
```

**Agent Reverb Vocabulary**:
```python
REVERB_PRESETS = {
    "small room": {"roomSize": 0.2, "damping": 0.7, "wet": 0.15},
    "medium room": {"roomSize": 0.4, "damping": 0.5, "wet": 0.25},
    "large hall": {"roomSize": 0.8, "damping": 0.3, "wet": 0.35},
    "cathedral": {"roomSize": 1.0, "damping": 0.2, "wet": 0.4},
    "plate": {"roomSize": 0.6, "damping": 0.6, "wet": 0.3, "width": 1.0},
    "vocal ambience": {"roomSize": 0.3, "damping": 0.5, "wet": 0.2, "preDelay": 20},
    "distant": {"roomSize": 0.7, "damping": 0.4, "wet": 0.5},
    "subtle": {"roomSize": 0.3, "damping": 0.6, "wet": 0.1},
}
```

#### 4.2.5 Delay

```cpp
class Delay : public Effect {
    float delayTimeMs = 250.0f;   // 1 to 2000 ms
    float feedback = 0.3f;         // 0 to 0.95 (NOT 1.0 - prevents infinite)
    float wetLevel = 0.3f;         // 0 to 1
    float dryLevel = 1.0f;         // 0 to 1
    bool pingPong = false;         // Stereo ping-pong mode
    float filterFreq = 8000.0f;    // Low-pass on feedback path
    
    // Implementation: Circular buffer with interpolation
};
```

#### 4.2.6 Saturation

```cpp
class Saturation : public Effect {
    float drive = 0.3f;           // 0 to 1
    SaturationType type = TAPE;
    float mix = 0.5f;             // 0 (dry) to 1 (fully saturated)
    float outputGain = 0.0f;      // Compensate for volume increase
    
    enum SaturationType { TAPE, TUBE, TRANSISTOR, HARD_CLIP };
    
    // Implementation: Waveshaping functions
    // TAPE: soft saturation with asymmetry
    // TUBE: even harmonics emphasis
    // TRANSISTOR: odd harmonics, harder edge
    // HARD_CLIP: digital clipping (use sparingly)
};
```

#### 4.2.7 Gate

```cpp
class Gate : public Effect {
    float thresholdDb = -40.0f;   // -80 to 0 dB
    float attackMs = 1.0f;         // 0.1 to 50 ms
    float releaseMs = 50.0f;       // 10 to 500 ms
    float holdMs = 10.0f;          // 0 to 100 ms
    float range = -80.0f;          // How much to attenuate (-80 = full gate)
    
    // Implementation: Envelope follower with hysteresis
};
```

#### 4.2.8 Limiter

```cpp
class Limiter : public Effect {
    float ceilingDb = -1.0f;      // -12 to 0 dB
    float releaseMs = 100.0f;     // 10 to 1000 ms
    bool truePeak = true;         // Intersample peak detection
    
    // Implementation: Brickwall limiter with lookahead
};
```

### 4.3 Effect Chain Rules

**Order Matters**: Effects are processed in chain order (index 0 first)

**Recommended Default Order** (agent should follow unless user specifies):
```
1. Gate (clean up noise before processing)
2. EQ (corrective) - remove problems
3. Compression
4. EQ (creative) - add color  
5. Saturation
6. Delay
7. Reverb (almost always last among time-based)
8. Limiter (always last)
```

**Agent Order Logic**:
```python
def get_recommended_position(effect_type: str, current_chain: list) -> int:
    """
    Returns the index where a new effect should be inserted.
    """
    ORDER_PRIORITY = {
        "gate": 0,
        "eq": 1,  # Will be placed before or after compression contextually
        "compressor": 2,
        "saturation": 3,
        "delay": 4,
        "reverb": 5,
        "limiter": 6
    }
    
    priority = ORDER_PRIORITY.get(effect_type, 3)  # Default to middle
    
    for i, effect in enumerate(current_chain):
        existing_priority = ORDER_PRIORITY.get(effect["type"], 3)
        if existing_priority > priority:
            return i
    
    return len(current_chain)  # Append at end
```

---

## 5. AI Agent Architecture <a name="ai-agent"></a>

### 5.1 Dual-Mode Operation

The AI agent has access to TWO tools:

```
┌────────────────────────────────────────────────────────────────┐
│                        AI AGENT                                 │
│                   (LLM-based reasoning)                         │
├────────────────────────────────────────────────────────────────┤
│                                                                 │
│   User Prompt ──────────────────────────────┐                  │
│                                              │                  │
│                                              ▼                  │
│                                    ┌─────────────────┐          │
│                                    │ Intent Analysis │          │
│                                    └────────┬────────┘          │
│                                              │                  │
│                          ┌───────────────────┼───────────────┐  │
│                          │                   │               │  │
│                          ▼                   ▼               ▼  │
│                    ┌──────────┐       ┌───────────┐    ┌──────┐│
│                    │ DSP Tool │       │Neural Tool│    │ Both ││
│                    └────┬─────┘       └─────┬─────┘    └──┬───┘│
│                         │                   │             │    │
│                         ▼                   ▼             │    │
│                    Layer 2 Params      Layer 1 Audio      │    │
│                                                           │    │
│                         └───────────────────┴─────────────┘    │
└────────────────────────────────────────────────────────────────┘
```

### 5.2 Tool Definitions

#### DSP Tool
```python
@dataclass
class DSPTool:
    """
    Manipulates Layer 2 effect chain parameters.
    Instant, non-destructive, infinitely adjustable.
    """
    
    capabilities = [
        "Add/remove/modify effects",
        "Adjust any DSP parameter",
        "Reorder effect chain",
        "Enable/disable effects"
    ]
    
    use_when = [
        "Standard mixing tasks (EQ, compression, reverb)",
        "User asks for specific effect by name",
        "Problem can be solved with traditional signal processing",
        "User wants immediate, tweakable results"
    ]
    
    advantages = [
        "Instant preview",
        "User can fine-tune after",
        "Deterministic output",
        "Low CPU cost"
    ]
```

#### Neural Tool
```python
@dataclass  
class NeuralTool:
    """
    Invokes AI/ML models to regenerate Layer 1.
    Slower, more transformative, requires re-render.
    """
    
    capabilities = [
        "Style transfer (make it sound like X)",
        "Noise reduction (AI denoising)",
        "Audio restoration (fix damaged audio)",
        "Dramatic transformation (genre shift, reimagine)"
    ]
    
    use_when = [
        "Holistic 'vibe' changes that can't be parameterized",
        "User references a style, era, or recording",
        "Problem requires understanding audio content semantically",
        "Traditional DSP would require many interconnected changes"
    ]
    
    limitations = [
        "Takes several seconds to process",
        "Result may vary (non-deterministic)",
        "Harder to fine-tune specific aspects",
        "Requires GPU for speed"
    ]
```

### 5.3 Available Neural Models

**Runtime Considerations**:
- **PyTorch**: Primary runtime for ACE-Step 1.5 and most neural models
- **ONNX Runtime**: For optimized inference of smaller models (denoise, restore)
- **SageAttention**: Optional optimization for transformer attention (2x speedup if available)
- **Model Loading**: See Section 17 (AI Bridge Architecture) for daemon/caching strategy

```python
NEURAL_MODEL_REGISTRY = {
    "ace-step": {
        "description": "ACE-Step 1.5 - Full music transformation via Hybrid Reasoning-Diffusion",
        "version": "1.5",
        "paper": "arXiv:2602.00744v1 (Jan 2026)",
        "architecture": {
            "dit_params": "2B parameters",
            "lm_planner": "Qwen3-0.6B (prompt expansion, CoT reasoning)",
            "vae": "1D VAE, 48kHz stereo, 64-dim latent @ 25Hz, 1920x compression",
            "inference_steps": "4-8 (distilled from 50)",
            "quantizer": "FSQ tokenizer, 5Hz discrete codes, ~64k codebook"
        },
        "capabilities": ["text_to_music", "cover", "repaint", "style_change", 
                        "track_extraction", "layering", "completion"],
        "inputs": {
            "mode": {"enum": ["transform", "repaint", "cover", "extract", "layer", "complete"]},
            "prompt": "text description of desired output",
            "preserve_melody": {"type": "bool", "default": True},
            "region_start": "optional float, start time for partial processing",
            "region_end": "optional float, end time for partial processing",
            "reference_audio": "optional path to style reference",
            "intensity": {"type": "float", "min": 0, "max": 1, "default": 0.7}
        },
        "runtime": {
            "framework": "PyTorch",
            "vram_requirement": "4GB minimum, 8GB recommended",
            "inference_time": {
                "A100": "~1-2 seconds per 4-minute song",
                "RTX 4090": "~5 seconds per 4-minute song",
                "RTX 3090": "~10 seconds per 4-minute song",
                "RTX 3060": "~30 seconds per 4-minute song"
            },
            "quantization": "FP16 default, INT8 available for <4GB VRAM"
        },
        "use_when": "Dramatic transformation, genre change, cover generation, reimagine as X",
        "known_worst_case_artifacts": [
            "Vocal intelligibility loss on complex lyrics (>50 words/minute)",
            "Tempo drift on pieces >5 minutes without strong rhythmic anchor",
            "Harmonic smearing on dense orchestral passages",
            "Transient softening on aggressive percussion (snare, kick)",
            "Occasional phantom vocals on instrumental-only requests",
            "Key drift on long-form generations without explicit key constraint"
        ]
    },
    
    "style-transfer": {
        "description": "Transfer timbral characteristics from reference or preset",
        "version": "1.0",
        "architecture": "Encoder-decoder with style embedding, ~500M params",
        "capabilities": ["timbre", "texture", "coloration", "vintage_simulation"],
        "inputs": {
            "reference_audio": "Optional path to style reference",
            "style_preset": {"enum": [
                "vintage_analog", "lo_fi", "modern_clean", "tape_warmth",
                "vinyl_crackle", "tube_console", "transistor_radio",
                "abbey_road_60s", "motown", "80s_digital", "90s_grunge"
            ]},
            "intensity": {"type": "float", "min": 0, "max": 1, "default": 0.5}
        },
        "runtime": {
            "framework": "ONNX Runtime (preferred) or PyTorch",
            "vram_requirement": "2GB",
            "inference_time": "3-10 seconds"
        },
        "use_when": "Holistic sound transformation, vintage vibes, 'sounds like X'",
        "known_worst_case_artifacts": [
            "High-frequency aliasing on 'vinyl' preset at >16kHz content",
            "Pumping artifacts when source already has heavy compression",
            "Stereo image collapse when using mono reference",
            "Noise floor increase with 'tape_warmth' preset (~3dB)",
            "Comb filtering with 'tube_console' on phase-sensitive material"
        ]
    },
    
    "denoise": {
        "description": "AI-based noise reduction preserving signal (DeepFilterNet-based)",
        "version": "3.0",
        "architecture": "Recurrent neural network with spectral masking",
        "capabilities": ["noise_removal", "hiss_removal", "hum_removal", "room_tone_reduction"],
        "inputs": {
            "strength": {"type": "float", "min": 0, "max": 1, "default": 0.5},
            "preserve_transients": {"type": "bool", "default": True},
            "noise_type": {"enum": ["auto", "broadband", "tonal", "impulse"]}
        },
        "runtime": {
            "framework": "ONNX Runtime",
            "vram_requirement": "1GB (can run on CPU efficiently)",
            "inference_time": "2-5 seconds (real-time capable)"
        },
        "use_when": "Noise, hiss, hum, cleanup, clarity issues",
        "known_worst_case_artifacts": [
            "Musical noise (twinkling/warbling) at strength >0.8",
            "Reverb tail truncation in quiet passages",
            "Sibilance dulling on speech at high strength",
            "Bass loss with 'tonal' mode on low-frequency content (<100Hz)",
            "Transient smearing on percussive content at high strength"
        ]
    },
    
    "restore": {
        "description": "Audio restoration for damaged/degraded recordings",
        "version": "1.0",
        "architecture": "U-Net with multi-task heads, ~300M params",
        "capabilities": ["declip", "dehum", "declick", "decrackle", "bandwidth_extension"],
        "inputs": {
            "mode": {"enum": ["auto", "declip", "dehum", "declick", "decrackle", "extend_bandwidth"]},
            "aggressiveness": {"type": "float", "min": 0, "max": 1, "default": 0.5}
        },
        "runtime": {
            "framework": "PyTorch",
            "vram_requirement": "2GB",
            "inference_time": "3-8 seconds"
        },
        "use_when": "Clipping, distortion, pops, clicks, old recordings, low quality",
        "known_worst_case_artifacts": [
            "Over-smoothing of intentional distortion (guitar overdrive, synth grit)",
            "Ghost transients in declip mode on sustained notes",
            "Harmonic generation in bandwidth extension sounds synthetic",
            "60Hz hum removal can affect bass guitar fundamentals",
            "Decrackle can remove intentional vinyl texture"
        ]
    },
    
    "enhance": {
        "description": "AI upsampling, clarity enhancement, presence boost",
        "version": "1.0",
        "architecture": "Transformer-based audio super-resolution",
        "capabilities": ["clarity", "fullness", "presence", "stereo_width", "upsample"],
        "inputs": {
            "target": {"enum": ["clarity", "fullness", "presence", "width", "all"]},
            "amount": {"type": "float", "min": 0, "max": 1, "default": 0.3}
        },
        "runtime": {
            "framework": "PyTorch",
            "vram_requirement": "3GB",
            "inference_time": "5-15 seconds"
        },
        "use_when": "Improve overall quality, add presence, enhance clarity",
        "known_worst_case_artifacts": [
            "Artificial 'sparkle' at amounts >0.7",
            "Phase issues in stereo width enhancement",
            "Harshness in presence mode on already bright sources",
            "Pumping in fullness mode on dynamic material",
            "Aliasing on already high-frequency content with upsample"
        ],
        "caution": "Start at 0.3, increase gradually"
    }
}
```

### 5.4 Neural Context Awareness

**Critical Issue**: If Neural processing adds intentional artifacts (e.g., vinyl crackle), the DSP chain must NOT accidentally remove them.

```python
class NeuralContextTracker:
    """
    Tracks what the Neural layer did so Agent makes informed DSP decisions.
    Prevents: Gate removing vinyl crackle, EQ "fixing" intentional lo-fi, etc.
    """
    
    def __init__(self):
        self.last_neural_operation = None
        self.intentional_artifacts = []
    
    def record_neural_operation(self, model: str, params: dict):
        self.last_neural_operation = {
            "model": model,
            "params": params,
            "timestamp": datetime.now().isoformat()
        }
        self.intentional_artifacts = self._detect_intentional_artifacts(model, params)
    
    def _detect_intentional_artifacts(self, model: str, params: dict) -> List[str]:
        artifacts = []
        
        if model == "style-transfer":
            preset = params.get("style_preset", "")
            if "vinyl" in preset:
                artifacts.extend(["high_frequency_noise", "frequency_rolloff", "subtle_crackle"])
            if "tape" in preset:
                artifacts.extend(["subtle_hiss", "saturation", "high_freq_rolloff"])
            if "lo_fi" in preset:
                artifacts.extend(["bitcrushing", "sample_rate_artifacts", "noise"])
            if "transistor" in preset:
                artifacts.extend(["bandwidth_limitation", "distortion"])
        
        if model == "ace-step":
            if params.get("mode") == "cover":
                artifacts.append("different_timbre")  # Don't "correct" the new voice
            if "vintage" in params.get("prompt", "").lower():
                artifacts.extend(["intentional_coloration", "frequency_rolloff"])
        
        return artifacts
    
    def get_dsp_warnings(self) -> List[str]:
        """Warnings for Agent about what NOT to do."""
        warnings = []
        
        if "high_frequency_noise" in self.intentional_artifacts:
            warnings.append("DO NOT add a noise gate - vinyl crackle is intentional")
        if "subtle_hiss" in self.intentional_artifacts:
            warnings.append("DO NOT use high shelf cut to remove hiss - tape hiss is intentional")
        if "different_timbre" in self.intentional_artifacts:
            warnings.append("DO NOT EQ to 'correct' timbre - it's a cover with new character")
        if "saturation" in self.intentional_artifacts:
            warnings.append("DO NOT use restoration/declip - saturation is intentional")
        if "frequency_rolloff" in self.intentional_artifacts:
            warnings.append("DO NOT boost highs to 'fix' rolloff - vintage sound is intentional")
        
        return warnings
```

### 5.5 Audio Analysis Protocol

**The Agent needs audio characteristics before making decisions.**

```cpp
// C++ AudioAnalyzer - called via CLI: ./nueva --analyze audio.wav --format json

struct AudioAnalysis {
    // Loudness (EBU R128)
    float rms_db;
    float peak_db;
    float true_peak_db;
    float lufs_integrated;
    float lufs_short_term;
    float lufs_momentary_max;
    float dynamic_range_db;      // LRA
    
    // Clipping
    float clip_percentage;
    int clip_region_count;
    std::vector<std::pair<float, float>> clip_locations;  // start, end times
    
    // Spectral
    float spectral_centroid_hz;
    float spectral_rolloff_hz;   // 85% energy below this
    float spectral_flatness;     // 0=tonal, 1=noisy
    std::array<float, 32> third_octave_db;  // 1/3 octave band levels
    
    // Stereo (if stereo)
    float stereo_correlation;    // -1 to 1
    float stereo_width;          // 0 to 1
    float balance;               // -1(L) to 1(R)
    bool has_phase_issues;       // correlation < 0.3
    
    // Noise/Quality
    float noise_floor_db;
    bool has_dc_offset;
    float dc_offset_value;
    
    // Musical (optional)
    float estimated_bpm;
    std::string estimated_key;
    float key_confidence;
    
    std::string toJson() const;
    std::string toHumanSummary() const;
};

// Human summary for agent prompt:
std::string AudioAnalysis::toHumanSummary() const {
    std::vector<std::string> issues;
    
    if (clip_percentage > 0.1)
        issues.push_back("⚠️ CLIPPING: " + std::to_string(clip_percentage) + "% samples clipped");
    if (lufs_integrated > -9)
        issues.push_back("⚠️ VERY LOUD: " + std::to_string(lufs_integrated) + " LUFS - likely over-limited");
    if (lufs_integrated < -20)
        issues.push_back("Quiet: " + std::to_string(lufs_integrated) + " LUFS - may need gain");
    if (noise_floor_db > -50)
        issues.push_back("Noisy: " + std::to_string(noise_floor_db) + " dB noise floor");
    if (has_dc_offset)
        issues.push_back("DC offset present - recommend HP filter");
    if (has_phase_issues)
        issues.push_back("⚠️ Phase issues detected (correlation: " + std::to_string(stereo_correlation) + ")");
    if (spectral_centroid_hz > 4000)
        issues.push_back("Bright/harsh character");
    else if (spectral_centroid_hz < 1500)
        issues.push_back("Dark/warm character");
    
    return issues.empty() ? "Audio appears healthy" : join(issues, "\n");
}
```

```python
# Python: Include analysis in agent requests

def prepare_agent_request(prompt: str, project: Project) -> dict:
    # Run analysis on current Layer 1
    analysis_result = subprocess.run(
        ["nueva", "--analyze", project.layer1_path, "--format", "json"],
        capture_output=True, text=True
    )
    analysis = json.loads(analysis_result.stdout)
    
    return {
        "action": "process",
        "prompt": prompt,
        "audio_analysis": analysis,
        "audio_summary": analysis["human_summary"],
        "neural_context": project.neural_context.get_dsp_warnings(),
        "current_dsp_state": project.get_dsp_chain_json()
    }
```

### 5.6 Neural Blend (Dry/Wet for AI)

**User often wants "30% of that vintage vibe" rather than 100%.**

```python
class NeuralBlendConfig:
    """
    Mix original (Layer 0) with AI-processed (Layer 1) before DSP chain.
    """
    enabled: bool = False
    blend: float = 1.0  # 0.0 = 100% original, 1.0 = 100% AI processed
    
    # Advanced: per-frequency blending
    crossover_enabled: bool = False
    crossover_freq_hz: float = 500.0  # Below: use original, Above: use AI
    crossover_blend_low: float = 0.3
    crossover_blend_high: float = 1.0
```

```cpp
// In audio rendering pipeline:

void renderWithNeuralBlend(const Project& project, AudioBuffer& output) {
    if (!project.neural_blend.enabled || project.neural_blend.blend >= 0.999f) {
        // No blending - just use Layer 1
        output = project.layer1.getAudio();
    }
    else if (project.neural_blend.blend <= 0.001f) {
        // Bypass neural - use Layer 0
        output = project.layer0.getAudio();
    }
    else {
        // Blend
        AudioBuffer layer0 = project.layer0.getAudio();
        AudioBuffer layer1 = project.layer1.getAudio();
        float blend = project.neural_blend.blend;
        
        if (project.neural_blend.crossover_enabled) {
            // Frequency-dependent blend
            AudioBuffer lowL0 = lowPass(layer0, project.neural_blend.crossover_freq_hz);
            AudioBuffer lowL1 = lowPass(layer1, project.neural_blend.crossover_freq_hz);
            AudioBuffer highL0 = highPass(layer0, project.neural_blend.crossover_freq_hz);
            AudioBuffer highL1 = highPass(layer1, project.neural_blend.crossover_freq_hz);
            
            output = mix(lowL0, lowL1, project.neural_blend.crossover_blend_low)
                   + mix(highL0, highL1, project.neural_blend.crossover_blend_high);
        }
        else {
            // Simple linear blend
            for (int i = 0; i < output.getNumSamples(); i++) {
                output[i] = layer0[i] * (1.0f - blend) + layer1[i] * blend;
            }
        }
    }
    
    // Now apply DSP chain
    project.dspChain.process(output);
}
```

### 5.7 Agent Decision Algorithm

```python
def decide_tool(prompt: str, context: ProjectContext) -> ToolDecision:
    """
    Main decision logic for tool selection.
    """
    
    # Step 1: Extract intent from prompt
    intent = analyze_intent(prompt)
    
    # Step 2: Check for explicit tool requests
    if intent.explicit_dsp_request:
        # User said "add an EQ" or "compress it"
        return ToolDecision(tool="dsp", confidence=0.95)
    
    if intent.explicit_neural_request:
        # User said "use AI to..." or "neural process..."
        return ToolDecision(tool="neural", confidence=0.95)
    
    # Step 3: Check for neural-only capabilities
    if requires_neural(intent):
        # Style transfer, restoration, semantic understanding
        return ToolDecision(tool="neural", confidence=0.85)
    
    # Step 4: Check if DSP can handle it
    if dsp_can_handle(intent):
        # Standard mixing tasks
        return ToolDecision(tool="dsp", confidence=0.80)
    
    # Step 5: Complex requests might need both
    if intent.is_complex:
        return ToolDecision(tool="both", confidence=0.70)
    
    # Step 6: Ambiguous - prefer DSP (faster, more control)
    return ToolDecision(tool="dsp", confidence=0.50, ask_clarification=True)


def requires_neural(intent: Intent) -> bool:
    """
    Check if the request fundamentally requires neural processing.
    """
    neural_indicators = [
        "sound like",           # Style transfer
        "as if recorded",       # Environment transfer
        "make it sound like a", # Style reference
        "vintage",              # Often needs holistic change
        "old recording",        # Restoration or style
        "remove noise",         # AI denoising
        "fix the clipping",     # Restoration
        "restore",              # Restoration
        "reimagine",            # Creative transformation
        "transform into",       # Genre shift
        "in the style of"       # Style transfer
    ]
    
    return any(indicator in intent.prompt_lower for indicator in neural_indicators)


def dsp_can_handle(intent: Intent) -> bool:
    """
    Check if standard DSP effects can achieve the goal.
    """
    dsp_indicators = [
        "eq", "equalize", "equalizer",
        "compress", "compressor", "compression",
        "reverb", "echo", "delay",
        "louder", "quieter", "volume", "gain",
        "bass", "treble", "mids", "highs", "lows",
        "brighter", "darker",  # Simple EQ
        "punchier", "punch",   # Compression
        "limit", "limiter",
        "gate", "noise gate"
    ]
    
    return any(indicator in intent.prompt_lower for indicator in dsp_indicators)
```

---

## 6. Agent Decision Logic (Detailed Scenarios) <a name="agent-decisions"></a>

### 6.1 Decision Matrix

| User Says | Tool | Effects/Model | Parameters | Confidence |
|-----------|------|---------------|------------|------------|
| "make it louder" | DSP | Gain | +6dB | 95% |
| "add some compression" | DSP | Compressor | gentle preset | 90% |
| "make it warmer" | DSP | EQ | low shelf +2, high shelf -2 | 75% |
| "make it sound vintage" | Neural | style-transfer | vintage_analog preset | 85% |
| "remove the background noise" | Neural | denoise | strength=0.7 | 90% |
| "make it punchier" | DSP | Compressor + EQ | fast attack, low-mid boost | 80% |
| "make it sound like a 60s recording" | Neural | style-transfer | + vinyl preset | 90% |
| "fix the clipping" | Neural | restore | declip mode | 95% |
| "improve it" | ASK | - | - | 20% |
| "make it sound better" | ASK | - | - | 20% |
| "add an EQ and make it vintage" | BOTH | EQ + style-transfer | - | 85% |
| "add 3dB at 1kHz" | DSP | EQ | exact params | 99% |
| "compress with 4:1 ratio" | DSP | Compressor | exact params | 99% |

### 6.2 Ambiguous Requests Handling

When confidence is low, agent should ask clarifying questions:

```python
CLARIFICATION_TEMPLATES = {
    "improve": "I'd be happy to help improve this! Could you tell me more about what aspect you'd like to focus on? For example:\n- Clarity and presence\n- Warmth and fullness\n- Punch and energy\n- Noise reduction\n- Overall polish",
    
    "better": "What would 'better' mean for this track? I can help with:\n- Tonal balance (EQ)\n- Dynamics (compression)\n- Space (reverb)\n- Noise issues\n- Or a complete sonic makeover",
    
    "fix": "What specifically needs fixing? I can help with:\n- Noise or hiss\n- Clipping/distortion\n- Muddy or harsh frequencies\n- Lack of punch or presence",
    
    "warmer_ambiguous": "When you say 'warmer', do you mean:\n1. Gentle high-frequency roll-off and bass boost (quick EQ adjustment)\n2. Analog-style saturation and character (subtle processing)\n3. Full vintage transformation (AI style transfer)\n\nI'd recommend starting with option 1 - it's quick and you can always go further."
}
```

### 6.3 Confidence Thresholds

```python
CONFIDENCE_THRESHOLDS = {
    "auto_execute": 0.80,      # Just do it, report what was done
    "suggest_first": 0.60,     # "I'm going to add X, sound good?"
    "ask_clarification": 0.40, # "Could you tell me more about..."
    "refuse_gracefully": 0.20  # "I'm not sure what you mean..."
}

def handle_confidence(decision: ToolDecision, context: Context) -> AgentResponse:
    if decision.confidence >= CONFIDENCE_THRESHOLDS["auto_execute"]:
        # High confidence - execute and report
        result = execute_decision(decision)
        return AgentResponse(
            action="executed",
            message=f"Done! I {describe_action(decision)}.",
            changes=result.changes
        )
    
    elif decision.confidence >= CONFIDENCE_THRESHOLDS["suggest_first"]:
        # Medium confidence - propose first
        return AgentResponse(
            action="propose",
            message=f"I'm thinking of {describe_action(decision)}. Should I go ahead?",
            proposed_changes=decision.changes
        )
    
    elif decision.confidence >= CONFIDENCE_THRESHOLDS["ask_clarification"]:
        # Low confidence - ask for more info
        return AgentResponse(
            action="clarify",
            message=get_clarification_question(decision.intent),
            proposed_changes=None
        )
    
    else:
        # Very low confidence - admit uncertainty
        return AgentResponse(
            action="uncertain",
            message="I'm not quite sure what you're looking for. Could you describe what you want to achieve in different words?",
            proposed_changes=None
        )
```

### 6.4 User Override Handling

User can always override agent decisions:

| User Says | Agent Behavior |
|-----------|----------------|
| "no, use the neural model instead" | Switch to neural tool |
| "actually just use EQ" | Switch to DSP/EQ only |
| "do it anyway" | Execute despite low confidence |
| "cancel" / "never mind" | Abort, no changes |
| "I want to do it manually" | Abort, provide guidance |
| "force [tool]" | Use specified tool regardless |

### 6.5 Relative vs Absolute Requests

| Request Type | Example | Handling |
|--------------|---------|----------|
| Absolute | "Set gain to -6dB" | Apply exact value |
| Absolute | "Compress at 4:1" | Apply exact ratio |
| Relative | "Make it louder" | Increase by reasonable amount (+3-6dB) |
| Relative | "More compression" | Decrease threshold or increase ratio |
| Relative | "A bit brighter" | Small high shelf boost (+2dB) |
| Relative | "Much brighter" | Larger high shelf boost (+6dB) |
| Comparative | "As loud as possible" | Maximize without clipping |
| Comparative | "Less compressed than before" | Reduce from current settings |

**Relative Amount Interpretation**:
```python
INTENSITY_MODIFIERS = {
    # Small
    "a bit", "slightly", "a little", "a touch", "subtly": 0.3,
    
    # Medium (default)
    "some", "more", "": 0.5,
    
    # Large
    "much", "a lot", "significantly", "considerably": 0.7,
    
    # Extreme
    "extremely", "very", "heavily", "drastically": 0.9
}

def interpret_relative_amount(prompt: str) -> float:
    """Returns 0-1 intensity multiplier"""
    prompt_lower = prompt.lower()
    
    for modifier, intensity in INTENSITY_MODIFIERS.items():
        if modifier and modifier in prompt_lower:
            return intensity
    
    return 0.5  # Default to medium
```

---

## 7. Conversation & Context Management <a name="conversation"></a>

### 7.1 Conversation State

The agent maintains conversation context within a session:

```python
@dataclass
class ConversationContext:
    session_id: str
    messages: List[Message]           # Full conversation history
    current_project: ProjectState     # Audio + layers + effects
    recent_actions: List[AgentAction] # What agent did recently
    user_preferences: Dict            # Learned from conversation
    
    # Derived context
    @property
    def last_action(self) -> Optional[AgentAction]:
        return self.recent_actions[-1] if self.recent_actions else None
    
    @property
    def effects_mentioned(self) -> Set[str]:
        """Effects the user has referenced in conversation"""
        # Used for "that EQ" / "the compressor" resolution
        pass
```

### 7.2 Reference Resolution

User often refers to previous context:

| User Says | Interpretation |
|-----------|----------------|
| "undo that" | Undo last agent action |
| "do that again" | Repeat last action |
| "more of that" | Intensify last action |
| "less of that" | Reduce last action intensity |
| "the EQ" | Most recently mentioned/added EQ |
| "that compressor" | Most recently mentioned/added compressor |
| "the first effect" | Effect at index 0 in chain |
| "the last one" | Most recently added effect |
| "remove it" | Remove last mentioned/added effect |
| "what did you do?" | Explain last action |

```python
def resolve_reference(ref: str, context: ConversationContext) -> Optional[Effect]:
    """
    Resolve ambiguous references like 'that', 'it', 'the EQ'
    """
    ref_lower = ref.lower()
    
    # Specific effect type reference
    effect_types = ["eq", "compressor", "reverb", "delay", "gate", "limiter", "saturation"]
    for effect_type in effect_types:
        if effect_type in ref_lower:
            # Find most recent effect of this type
            return find_most_recent_effect(context, effect_type)
    
    # Ordinal reference
    if "first" in ref_lower:
        return context.current_project.dsp_chain[0] if context.current_project.dsp_chain else None
    if "last" in ref_lower:
        return context.current_project.dsp_chain[-1] if context.current_project.dsp_chain else None
    
    # Generic reference - use last action
    if ref_lower in ["it", "that", "this"]:
        if context.last_action and context.last_action.affected_effect:
            return context.last_action.affected_effect
    
    return None
```

### 7.3 Multi-Turn Refinement

Typical refinement flow:

```
User: "Add some compression"
Agent: [Adds compressor with default preset]
       "Added a compressor with gentle settings (threshold -24dB, ratio 2:1)"

User: "Make it more aggressive"  
Agent: [Modifies SAME compressor]
       "Adjusted the compressor - now at -18dB threshold, 4:1 ratio, faster attack"

User: "Too much, back off a bit"
Agent: [Reduces SAME compressor]
       "Backed off a bit - threshold -21dB, ratio 3:1"

User: "Perfect. Now add some reverb"
Agent: [Adds NEW reverb effect]
       "Added reverb - medium room, 25% wet"
```

**Key Insight**: Agent must track which effect is "active" in conversation to modify it vs add new.

```python
class EffectFocus:
    """Tracks which effect the conversation is currently about"""
    effect_id: Optional[str] = None
    effect_type: Optional[str] = None
    since_message_index: int = 0
    
    def update(self, action: AgentAction, message_index: int):
        if action.action_type in ["add", "modify"]:
            self.effect_id = action.effect_id
            self.effect_type = action.effect_type
            self.since_message_index = message_index
    
    def should_modify_vs_add(self, prompt: str, effect_type: str) -> str:
        """
        Determine if we should modify the focused effect or add new.
        """
        modification_signals = [
            "more", "less", "increase", "decrease", "adjust",
            "too much", "not enough", "back off", "push it"
        ]
        
        if self.effect_type == effect_type:
            if any(sig in prompt.lower() for sig in modification_signals):
                return "modify"
        
        return "add"
```

### 7.4 Undo/Redo Stack

Every agent action is undoable:

```python
@dataclass
class UndoableAction:
    action_id: str
    timestamp: datetime
    description: str  # Human readable
    
    # State snapshots
    dsp_chain_before: List[Dict]
    dsp_chain_after: List[Dict]
    layer1_path_before: Optional[str]  # For neural actions
    layer1_path_after: Optional[str]
    
    def undo(self, project: Project):
        project.dsp_chain = deepcopy(self.dsp_chain_before)
        if self.layer1_path_before:
            project.layer1.restore_from(self.layer1_path_before)
    
    def redo(self, project: Project):
        project.dsp_chain = deepcopy(self.dsp_chain_after)
        if self.layer1_path_after:
            project.layer1.restore_from(self.layer1_path_after)


class UndoManager:
    undo_stack: List[UndoableAction] = []
    redo_stack: List[UndoableAction] = []
    max_undo_levels: int = 50
    
    def record_action(self, action: UndoableAction):
        self.undo_stack.append(action)
        self.redo_stack.clear()  # Redo invalidated by new action
        
        # Limit stack size
        if len(self.undo_stack) > self.max_undo_levels:
            self.undo_stack.pop(0)
    
    def undo(self, project: Project) -> Optional[str]:
        if not self.undo_stack:
            return "Nothing to undo"
        
        action = self.undo_stack.pop()
        action.undo(project)
        self.redo_stack.append(action)
        return f"Undone: {action.description}"
    
    def redo(self, project: Project) -> Optional[str]:
        if not self.redo_stack:
            return "Nothing to redo"
        
        action = self.redo_stack.pop()
        action.redo(project)
        self.undo_stack.append(action)
        return f"Redone: {action.description}"
```

### 7.5 Explanation Capability

User can always ask what the agent did:

```python
def explain_last_action(context: ConversationContext) -> str:
    action = context.last_action
    if not action:
        return "I haven't made any changes yet."
    
    explanation = f"I {action.description}.\n\n"
    
    if action.tool == "dsp":
        explanation += "Here's what changed:\n"
        for change in action.parameter_changes:
            explanation += f"  - {change.effect_name}: {change.param} went from {change.old_value} to {change.new_value}\n"
        
        explanation += f"\nI did this because: {action.reasoning}"
    
    elif action.tool == "neural":
        explanation += f"I used the {action.model_name} model to process your audio.\n"
        explanation += f"Settings: {format_model_params(action.model_params)}\n"
        explanation += f"\nThis was the right choice because: {action.reasoning}"
    
    return explanation


def explain_full_chain(project: Project) -> str:
    """Explain everything currently applied"""
    if not project.dsp_chain:
        return "No effects are currently applied. The audio is passing through clean."
    
    explanation = "Here's your current effect chain:\n\n"
    for i, effect in enumerate(project.dsp_chain, 1):
        explanation += f"{i}. {effect.display_name}"
        if not effect.enabled:
            explanation += " (bypassed)"
        explanation += "\n"
        explanation += f"   {effect.describe_settings()}\n"
    
    return explanation
```

---

## 8. State Management & Persistence <a name="state-management"></a>

### 8.1 Project File Structure

```
my_project.nueva/                      # Project is a directory
├── project.json                       # Master state file
├── audio/
│   ├── layer0_source.wav             # Original audio (immutable)
│   ├── layer1_ai.wav                 # Current AI-processed audio
│   └── layer1_ai_meta.json           # AI processing metadata
├── history/
│   ├── undo_stack.json               # Undo/redo state
│   └── action_log.json               # Full action history
├── backups/
│   ├── layer0_pre_bake_20240115.wav  # Pre-bake backup
│   └── autosave_20240115_143022.json # Periodic autosave
├── exports/
│   └── final_mix_20240115.wav        # User exports
└── cache/
    └── waveform.json                  # Cached waveform display data
```

### 8.2 project.json Schema

```json
{
  "version": "1.0.0",
  "created_at": "2024-01-15T10:30:00Z",
  "modified_at": "2024-01-15T14:25:00Z",
  "nueva_version": "0.1.0",
  
  "source": {
    "original_filename": "my_recording.wav",
    "original_path": "/Users/emilio/Music/my_recording.wav",
    "import_settings": {
      "converted_from_sample_rate": 44100,
      "converted_from_bit_depth": 16
    }
  },
  
  "layer0": {
    "path": "audio/layer0_source.wav",
    "sample_rate": 48000,
    "bit_depth": 32,
    "channels": 2,
    "duration_seconds": 180.5,
    "hash_sha256": "abc123..."
  },
  
  "layer1": {
    "path": "audio/layer1_ai.wav",
    "is_processed": true,
    "identical_to_layer0": false,
    "processing": {
      "model": "style-transfer",
      "prompt": "make it sound like a vintage analog recording",
      "params": {
        "style_preset": "tape_warmth",
        "intensity": 0.6
      },
      "processed_at": "2024-01-15T11:00:00Z",
      "processing_time_ms": 4520
    }
  },
  
  "layer2": {
    "chain": [
      {
        "id": "eq_abc123",
        "type": "parametric_eq",
        "enabled": true,
        "params": {
          "bands": [
            {"freq": 80, "gain": 3.0, "q": 0.7, "type": "lowshelf"},
            {"freq": 3000, "gain": 2.0, "q": 1.5, "type": "peak"}
          ]
        },
        "added_at": "2024-01-15T12:00:00Z",
        "added_by": "agent"
      },
      {
        "id": "comp_def456",
        "type": "compressor",
        "enabled": true,
        "params": {
          "threshold": -18,
          "ratio": 4.0,
          "attack_ms": 10,
          "release_ms": 100,
          "makeup_gain": 3.0
        },
        "added_at": "2024-01-15T12:05:00Z",
        "added_by": "agent"
      }
    ]
  },
  
  "conversation": {
    "session_count": 3,
    "last_session": "2024-01-15T14:00:00Z",
    "total_messages": 47,
    "user_preferences": {
      "prefers_dsp_first": true,
      "compression_preference": "gentle",
      "typical_genre": "jazz"
    }
  }
}
```

### 8.3 Autosave Strategy

```python
class AutosaveManager:
    autosave_interval_seconds = 60
    max_autosaves = 10
    
    def should_autosave(self, project: Project) -> bool:
        """
        Autosave if:
        - It's been > interval since last save
        - AND there are unsaved changes
        - AND we're not currently processing
        """
        time_since_save = now() - project.last_save_time
        return (
            time_since_save.seconds > self.autosave_interval_seconds
            and project.has_unsaved_changes
            and not project.is_processing
        )
    
    def autosave(self, project: Project):
        """Save state without overwriting main project file"""
        autosave_path = project.backups_dir / f"autosave_{timestamp()}.json"
        
        # Save just the state, not audio files
        save_state_json(project, autosave_path)
        
        # Rotate old autosaves
        autosaves = sorted(project.backups_dir.glob("autosave_*.json"))
        while len(autosaves) > self.max_autosaves:
            oldest = autosaves.pop(0)
            oldest.unlink()
```

### 8.4 Crash Recovery

```python
def recover_from_crash(project_path: Path) -> RecoveryResult:
    """
    Attempt to recover project state after unexpected termination.
    """
    project_dir = project_path if project_path.is_dir() else project_path.parent
    
    # Check for lock file (indicates crash)
    lock_file = project_dir / ".lock"
    if not lock_file.exists():
        return RecoveryResult(needed=False)
    
    # Find most recent autosave
    autosaves = sorted(project_dir.glob("backups/autosave_*.json"), reverse=True)
    
    if not autosaves:
        return RecoveryResult(
            needed=True,
            success=False,
            message="No autosave found. Project may have unsaved changes."
        )
    
    # Load autosave state
    latest_autosave = autosaves[0]
    autosave_time = parse_timestamp_from_filename(latest_autosave)
    
    return RecoveryResult(
        needed=True,
        success=True,
        message=f"Found autosave from {autosave_time}. Recover this state?",
        recovery_state_path=latest_autosave
    )
```

---

## 9. Error Handling Matrix <a name="error-handling"></a>

### 9.1 Error Categories

| Category | Example | Recovery Strategy |
|----------|---------|-------------------|
| **User Error** | Invalid prompt | Ask for clarification |
| **File Error** | File not found | Clear message, suggest fix |
| **Audio Error** | Corrupt audio | Attempt repair or reject |
| **Processing Error** | Effect causes NaN | Bypass effect, warn user |
| **AI Error** | Model fails | Fall back to DSP suggestion |
| **Resource Error** | Out of memory | Suggest smaller file or close other apps |
| **System Error** | Disk full | Warn, prevent data loss |

### 9.2 Complete Error Handling Specifications

```python
class NuevaError(Exception):
    """Base error class with recovery info"""
    error_code: str
    user_message: str
    technical_details: str
    recovery_suggestions: List[str]
    is_recoverable: bool


# File Errors
class FileNotFoundError(NuevaError):
    error_code = "FILE_NOT_FOUND"
    is_recoverable = True
    recovery_suggestions = [
        "Check the file path is correct",
        "Verify the file hasn't been moved or deleted",
        "Try importing from a different location"
    ]

class InvalidAudioFileError(NuevaError):
    error_code = "INVALID_AUDIO"
    is_recoverable = True
    recovery_suggestions = [
        "Try converting the file to WAV format first",
        "Check if the file plays in another application",
        "The file may be corrupted - try re-exporting from source"
    ]

class UnsupportedFormatError(NuevaError):
    error_code = "UNSUPPORTED_FORMAT"
    is_recoverable = True
    recovery_suggestions = [
        "Convert to WAV, AIFF, or FLAC format",
        "Supported formats: WAV, AIFF, FLAC, MP3, OGG"
    ]

# Processing Errors
class ProcessingError(NuevaError):
    error_code = "PROCESSING_ERROR"
    is_recoverable = True
    
class DSPOverflowError(ProcessingError):
    """Effect produced NaN or infinity"""
    error_code = "DSP_OVERFLOW"
    recovery_suggestions = [
        "The effect settings may be too extreme",
        "Try reducing the effect intensity",
        "Effect has been bypassed to prevent audio corruption"
    ]

class AIProcessingError(NuevaError):
    error_code = "AI_PROCESSING_ERROR"
    is_recoverable = True
    recovery_suggestions = [
        "Try a different AI model",
        "Use DSP effects instead for similar result",
        "Reduce audio length and try again"
    ]

class ModelNotFoundError(AIProcessingError):
    error_code = "MODEL_NOT_FOUND"
    recovery_suggestions = [
        "The requested model is not installed",
        "Run 'nueva install-model <model_name>' to install",
        "Available models: style-transfer, denoise, restore"
    ]

# Resource Errors
class OutOfMemoryError(NuevaError):
    error_code = "OUT_OF_MEMORY"
    is_recoverable = True
    recovery_suggestions = [
        "Close other applications to free memory",
        "Try processing a shorter audio segment",
        "Use CPU processing instead of GPU"
    ]

class DiskFullError(NuevaError):
    error_code = "DISK_FULL"
    is_recoverable = True
    recovery_suggestions = [
        "Free up disk space",
        "Change the project location to a drive with more space",
        "Export to a different location"
    ]

# Agent Errors
class AmbiguousPromptError(NuevaError):
    error_code = "AMBIGUOUS_PROMPT"
    is_recoverable = True
    # Not really an error - just needs clarification

class ConflictingRequestError(NuevaError):
    error_code = "CONFLICTING_REQUEST"
    is_recoverable = True
    recovery_suggestions = [
        "Your request contains conflicting goals",
        "Try splitting into separate requests"
    ]
```

### 9.3 Error Response Templates

```python
ERROR_RESPONSES = {
    "FILE_NOT_FOUND": {
        "friendly": "I couldn't find the file at '{path}'. Could you check if it's in the right location?",
        "suggestions": True
    },
    
    "INVALID_AUDIO": {
        "friendly": "This file doesn't appear to be valid audio, or it might be corrupted. What happened when you try to play it in another app?",
        "suggestions": True
    },
    
    "DSP_OVERFLOW": {
        "friendly": "Whoa, that effect setting created some problematic audio! I've bypassed it to protect your ears. Let's try more moderate settings.",
        "suggestions": False
    },
    
    "AI_PROCESSING_ERROR": {
        "friendly": "The AI processing didn't work this time. This sometimes happens with certain audio. Want me to try a DSP-based approach instead?",
        "suggestions": True,
        "offer_alternative": True
    },
    
    "MODEL_NOT_FOUND": {
        "friendly": "I don't have the '{model}' model available. Here's what I can use instead: {available_models}",
        "suggestions": False
    },
    
    "OUT_OF_MEMORY": {
        "friendly": "We're running low on memory for this operation. A few options:\n1. Close some other apps\n2. Process just a section of the audio\n3. Use lighter processing",
        "suggestions": False
    },
    
    "AMBIGUOUS_PROMPT": {
        "friendly": "{clarification_question}",
        "suggestions": False
    }
}
```

### 9.4 Defensive Processing

Every audio processing operation should be wrapped:

```cpp
class SafeProcessor {
public:
    ProcessResult processSafely(AudioBuffer& buffer, Effect& effect) {
        // Save state for rollback
        AudioBuffer backup = buffer.createCopy();
        
        try {
            effect.process(buffer);
            
            // Validate output
            if (!validateBuffer(buffer)) {
                buffer = backup;  // Rollback
                return ProcessResult::failure("Effect produced invalid audio");
            }
            
            return ProcessResult::success();
            
        } catch (const std::exception& e) {
            buffer = backup;  // Rollback
            return ProcessResult::failure(e.what());
        }
    }
    
private:
    bool validateBuffer(const AudioBuffer& buffer) {
        for (int ch = 0; ch < buffer.getNumChannels(); ch++) {
            const float* data = buffer.getReadPointer(ch);
            for (int i = 0; i < buffer.getNumSamples(); i++) {
                // Check for NaN
                if (std::isnan(data[i])) return false;
                // Check for infinity
                if (std::isinf(data[i])) return false;
                // Check for extreme values (> +24dBFS)
                if (std::abs(data[i]) > 16.0f) return false;
            }
        }
        return true;
    }
};
```

---

## 10. Edge Cases & Special Scenarios <a name="edge-cases"></a>

### 10.1 Audio Edge Cases

| Scenario | Detection | Handling |
|----------|-----------|----------|
| Silent audio | RMS < -80dBFS | Warn user, allow processing |
| Nearly silent | RMS < -60dBFS | Warn, suggest gain increase |
| Clipped audio | >1% samples at ±1.0 | Warn, suggest restoration |
| DC offset | Mean > 0.01 | Auto-remove with high-pass at 10Hz |
| Very short (<1s) | Duration check | Allow, warn about limitations |
| Very long (>30min) | Duration check | Warn about memory, proceed |
| Mono input | Channel count | Process as mono, don't fake stereo |
| Stereo imbalance | L/R RMS difference | Warn, don't auto-correct |
| Phase issues | Correlation < 0.3 | Warn if applying stereo effects |

### 10.2 Agent Edge Cases

| Scenario | User Says | Agent Behavior |
|----------|-----------|----------------|
| Contradictory request | "make it louder but also more dynamic" | Explain tradeoff, ask preference |
| Impossible request | "remove the vocals completely" | Explain limitation, suggest stem separation tool |
| Self-defeating | "add compression then undo compression" | Just don't add it, confirm |
| Already applied | "add an EQ" when EQ exists | Modify existing vs add second, ask preference |
| Excessive processing | "add 5 compressors" | Warn about quality degradation |
| Undo unavailable | "undo" with empty stack | "Nothing to undo yet" |
| Reference to nothing | "adjust that" with no context | "Which effect do you mean?" |
| Typos/misspellings | "add compresion" | Fuzzy match, confirm intent |
| Wrong terminology | "add more bass drum" (on non-drum audio) | Interpret as "boost low frequencies" |
| Jargon overload | "add parallel compression with NY-style sidechaining" | Do best interpretation, explain what was done |

### 10.3 Conflicting Goals

```python
CONFLICTING_PAIRS = [
    ("louder", "more dynamic range"),    # Can't have both
    ("warmer", "brighter"),               # Usually opposite
    ("more reverb", "more clarity"),      # Reverb reduces clarity
    ("compressed", "natural dynamics"),   # Direct opposites
    ("vintage", "crystal clear"),         # Usually conflicting aesthetics
]

def detect_conflicts(intents: List[str]) -> Optional[Conflict]:
    for a, b in CONFLICTING_PAIRS:
        if a in intents and b in intents:
            return Conflict(
                goals=[a, b],
                message=f"'{a}' and '{b}' are somewhat opposing goals. Which is more important to you?",
                suggestion=f"I can aim for a balance, or prioritize one over the other."
            )
    return None
```

### 10.4 User Frustration Handling

Detect and respond to user frustration:

```python
FRUSTRATION_SIGNALS = [
    "this isn't working",
    "you're not understanding",
    "no that's wrong",
    "I said",
    "why can't you",
    "just do it",
    "forget it",
    "start over",
    "ugh",
    "this is frustrating"
]

def detect_frustration(messages: List[Message]) -> bool:
    recent = messages[-3:]  # Last 3 messages
    text = " ".join(m.content.lower() for m in recent)
    return any(signal in text for signal in FRUSTRATION_SIGNALS)

def handle_frustration(context: Context) -> str:
    return """I sense this isn't going smoothly - I'm sorry about that. Let me try a different approach:

1. **Start fresh**: I can reset to your original audio
2. **Manual control**: I can show you exactly what each knob does
3. **Specific guidance**: Tell me ONE thing you want different about the sound

What would help most right now?"""
```

### 10.5 The "Actually..." Pattern

User says one thing, then immediately corrects:

```python
def detect_correction(messages: List[Message]) -> bool:
    if len(messages) < 2:
        return False
    
    latest = messages[-1].content.lower()
    correction_signals = [
        "actually", "wait", "no,", "I meant", "sorry,",
        "that's not", "I said", "instead"
    ]
    
    return any(latest.startswith(sig) or f" {sig}" in latest for sig in correction_signals)

def handle_correction(prompt: str, context: Context) -> AgentResponse:
    """
    User is correcting previous instruction. 
    Undo last action and reinterpret.
    """
    # First, undo what we just did
    undo_result = context.undo_manager.undo(context.project)
    
    # Extract the actual intent from the correction
    new_intent = extract_intent_from_correction(prompt)
    
    return AgentResponse(
        message=f"Got it, let me undo that. {undo_result}\n\nNow, {describe_new_interpretation(new_intent)}",
        action="reprocess"
    )
```

---

## 11. Multi-Track Architecture (Future Foundation) <a name="multi-track"></a>

While v1 is single-track, the architecture should support future multi-track:

### 11.1 Track Structure

```python
@dataclass
class Track:
    id: str
    name: str
    type: TrackType  # AUDIO, BUS, MASTER
    
    # Each track has its own layer system
    layer0: AudioFile     # Source audio (None for bus/master)
    layer1: AudioFile     # AI processed
    layer2: EffectChain   # DSP effects
    
    # Routing
    output_bus: str       # Where this track sends to
    sends: List[Send]     # Aux sends
    
    # Mix parameters
    volume: float         # -inf to +12 dB
    pan: float           # -1 (L) to +1 (R)
    muted: bool
    soloed: bool


class TrackType(Enum):
    AUDIO = "audio"       # Has source audio
    BUS = "bus"           # Sum of other tracks
    MASTER = "master"     # Final output
```

### 11.2 Track Reference in Prompts

```python
TRACK_REFERENCE_PATTERNS = [
    r"(?:the|on|to)?\s*(?:vocal|vocals|voice)\s*(?:track)?",
    r"(?:the|on|to)?\s*(?:drum|drums|percussion)\s*(?:track)?",
    r"(?:the|on|to)?\s*(?:bass)\s*(?:track)?",
    r"(?:the|on|to)?\s*(?:guitar)\s*(?:track)?",
    r"track\s*(\d+)",                    # "track 1"
    r"(?:the|on|to)?\s*\"([^\"]+)\"",   # quoted track name
    r"(?:the|on|to)?\s*master",         # master track
]

def resolve_track_reference(prompt: str, project: MultitrackProject) -> Optional[Track]:
    # Implementation: Match patterns and fuzzy-match track names
    pass
```

### 11.3 Multi-Track Agent Commands

| Command | Scope | Example |
|---------|-------|---------|
| "make it warmer" | Current track or all | Depends on context |
| "make the vocals warmer" | Vocal track | Clear reference |
| "add reverb to everything" | All tracks | Global |
| "compress the drums" | Drum track | Clear reference |
| "balance the mix" | Master | Adjust relative levels |
| "pan the guitars wider" | Guitar tracks | Stereo field |

---

## 12. Testing Strategy <a name="testing"></a>

### 12.1 Test Categories

```
tests/
├── unit/                    # Test individual components
│   ├── test_audio_io.cpp
│   ├── test_effects.cpp
│   ├── test_dsp_chain.cpp
│   └── test_layer_manager.cpp
├── integration/             # Test component interaction
│   ├── test_project_lifecycle.cpp
│   ├── test_ai_bridge.cpp
│   └── test_full_pipeline.cpp
├── agent/                   # Test agent logic (Python)
│   ├── test_intent_parsing.py
│   ├── test_tool_selection.py
│   ├── test_conversation.py
│   └── test_edge_cases.py
├── audio_quality/           # Audio validation tests
│   ├── test_passthrough.cpp
│   ├── test_effect_quality.cpp
│   └── test_no_artifacts.cpp
└── fixtures/                # Test audio files
    ├── generate_fixtures.sh
    └── README.md
```

### 12.2 Test Fixture Generation

```bash
#!/bin/bash
# generate_fixtures.sh - Create all test audio files

# Sine waves at various frequencies
for freq in 100 440 1000 10000; do
    sox -n fixtures/sine_${freq}hz.wav synth 2 sine $freq
done

# White noise
sox -n fixtures/white_noise.wav synth 2 whitenoise

# Pink noise (more musical)
sox -n fixtures/pink_noise.wav synth 2 pinknoise

# Silence
sox -n fixtures/silence.wav trim 0 2

# Clipped audio (for testing detection)
sox -n fixtures/clipped.wav synth 2 sine 440 gain 20

# Very quiet audio
sox -n fixtures/quiet.wav synth 2 sine 440 gain -60

# DC offset
sox -n fixtures/dc_offset.wav synth 2 sine 440 dcshift 0.1

# Stereo test (different L/R)
sox -n fixtures/stereo_test.wav synth 2 sine 440 sine 880 channels 2

# Simple chord progression (for musical tests)
sox -n fixtures/chord_progression.wav \
    synth 4 pl C3 pl E3 pl G3 fade 0 4 0.5 : \
    synth 4 pl F3 pl A3 pl C4 fade 0 4 0.5 : \
    synth 4 pl G3 pl B3 pl D4 fade 0 4 0.5 : \
    synth 4 pl C3 pl E3 pl G3 fade 0 4 0.5
```

### 12.3 Audio Quality Tests

```cpp
// Test that pass-through doesn't modify audio
TEST_CASE("Passthrough is bit-perfect") {
    auto input = loadAudioFile("fixtures/sine_440hz.wav");
    auto output = AudioBuffer(input);
    
    DSPChain emptyChain;
    emptyChain.process(output);
    
    REQUIRE(buffersAreIdentical(input, output));
}

// Test that effects don't introduce artifacts
TEST_CASE("EQ doesn't introduce noise") {
    auto silence = loadAudioFile("fixtures/silence.wav");
    
    EQEffect eq;
    eq.addBand(1000, 6.0, 1.0);  // +6dB at 1kHz
    eq.process(silence);
    
    // Silence should still be silence after EQ
    REQUIRE(calculateRMS(silence) < DB_TO_LINEAR(-80));
}

// Test that gain is accurate
TEST_CASE("Gain is accurate within 0.1dB") {
    auto input = loadAudioFile("fixtures/sine_440hz.wav");
    float inputRMS = calculateRMSdB(input);
    
    GainEffect gain;
    gain.setGaindB(6.0);
    gain.process(input);
    
    float outputRMS = calculateRMSdB(input);
    REQUIRE_THAT(outputRMS, WithinAbs(inputRMS + 6.0, 0.1));
}

// Test compressor reduces dynamic range
TEST_CASE("Compressor reduces dynamic range") {
    auto input = loadAudioFile("fixtures/chord_progression.wav");
    float inputCrest = calculateCrestFactor(input);
    
    Compressor comp;
    comp.setThreshold(-18);
    comp.setRatio(4);
    comp.process(input);
    
    float outputCrest = calculateCrestFactor(input);
    REQUIRE(outputCrest < inputCrest);  // Crest factor should decrease
}
```

### 12.4 Agent Test Suite

```python
# test_intent_parsing.py

import pytest
from nueva.agent import analyze_intent, decide_tool

class TestIntentParsing:
    
    @pytest.mark.parametrize("prompt,expected_tool", [
        ("make it louder", "dsp"),
        ("add compression", "dsp"),
        ("boost the bass", "dsp"),
        ("make it sound vintage", "neural"),
        ("remove the background noise", "neural"),
        ("add 3dB at 1kHz", "dsp"),
        ("make it sound like a 60s recording", "neural"),
    ])
    def test_tool_selection(self, prompt, expected_tool):
        decision = decide_tool(prompt, mock_context())
        assert decision.tool == expected_tool
    
    @pytest.mark.parametrize("prompt,expected_effect", [
        ("add an eq", "parametric_eq"),
        ("compress it", "compressor"),
        ("add some reverb", "reverb"),
        ("add delay", "delay"),
        ("make it louder", "gain"),
        ("limit the peaks", "limiter"),
    ])
    def test_effect_identification(self, prompt, expected_effect):
        decision = decide_tool(prompt, mock_context())
        assert expected_effect in [e["type"] for e in decision.dsp_changes]
    
    def test_ambiguous_prompt_asks_clarification(self):
        decision = decide_tool("make it better", mock_context())
        assert decision.ask_clarification == True
    
    def test_conflicting_request_detected(self):
        decision = decide_tool("make it louder and more dynamic", mock_context())
        assert decision.has_conflict == True


class TestConversationContext:
    
    def test_undo_reference(self):
        context = mock_context_with_history([
            ("add compression", {"tool": "dsp", "effect": "compressor"})
        ])
        decision = decide_tool("undo that", context)
        assert decision.action == "undo"
    
    def test_modification_reference(self):
        context = mock_context_with_history([
            ("add compression", {"tool": "dsp", "effect_id": "comp_1"})
        ])
        decision = decide_tool("more aggressive", context)
        assert decision.action == "modify"
        assert decision.target_effect_id == "comp_1"
    
    def test_that_reference_resolution(self):
        context = mock_context_with_history([
            ("add an EQ", {"effect_id": "eq_1"}),
            ("add compression", {"effect_id": "comp_1"}),
        ])
        decision = decide_tool("remove the EQ", context)
        assert decision.action == "remove"
        assert decision.target_effect_id == "eq_1"
```

### 12.5 Integration Tests

```bash
#!/bin/bash
# integration_tests.sh

set -e  # Exit on any failure

echo "=== Nueva Integration Tests ==="

# Setup
./nueva --generate-test-tone --output /tmp/test_tone.wav --freq 440 --duration 5

# Test 1: Basic project creation
echo "Test 1: Project creation..."
./nueva --input /tmp/test_tone.wav --create-project /tmp/test_project
test -f /tmp/test_project/project.json || exit 1
test -f /tmp/test_project/audio/layer0_source.wav || exit 1
echo "PASS"

# Test 2: DSP processing
echo "Test 2: DSP processing..."
./nueva --project /tmp/test_project --gain -6.0
GAIN_RESULT=$(./nueva --project /tmp/test_project --print-chain | grep "gain")
[[ "$GAIN_RESULT" == *"-6"* ]] || exit 1
echo "PASS"

# Test 3: AI bridge communication
echo "Test 3: AI bridge..."
BRIDGE_RESULT=$(echo '{"action":"ping"}' | python -m nueva.ai_bridge)
[[ "$BRIDGE_RESULT" == *"pong"* ]] || exit 1
echo "PASS"

# Test 4: Full pipeline
echo "Test 4: Full pipeline..."
./nueva --project /tmp/test_project --ai-prompt "make it warmer" --ai-model mock
./nueva --project /tmp/test_project --export /tmp/final_output.wav
test -f /tmp/final_output.wav || exit 1
echo "PASS"

# Test 5: Undo/redo
echo "Test 5: Undo/redo..."
./nueva --project /tmp/test_project --gain 6.0
./nueva --project /tmp/test_project --undo
CHAIN=$(./nueva --project /tmp/test_project --print-chain)
[[ "$CHAIN" != *"+6"* ]] || exit 1
echo "PASS"

echo "=== All tests passed ==="
```

### 12.6 Verification Without Listening

| Test Goal | Measurement | Tool |
|-----------|-------------|------|
| Gain accuracy | RMS level difference | sox stats |
| EQ effect | Spectral centroid shift | custom FFT |
| Compression | Crest factor reduction | sox stats |
| No clipping | Peak level < 0dBFS | sox stats |
| Duration preserved | Sample count | ffprobe |
| No artifacts | Noise floor unchanged | sox stats on silent portions |
| Passthrough integrity | File hash | md5sum |

---

## 13. Implementation Phases <a name="phases"></a>

### Phase 1: Audio Engine Foundation (Week 1-2)

**Goal**: CLI tool that loads audio, applies DSP chain, exports result.

#### Milestone 1.1: Project Skeleton
```bash
# Success criteria
mkdir build && cd build && cmake .. && make
./nueva --help  # Shows usage
```

#### Milestone 1.2: Audio File I/O
```bash
# Success criteria
./nueva --generate-test-tone --output test.wav --freq 440 --duration 2
./nueva --input test.wav --output copy.wav
diff <(md5sum test.wav) <(md5sum copy.wav)  # Pass-through test
```

#### Milestone 1.3: Gain Effect
```bash
# Success criteria
./nueva --input test.wav --output quiet.wav --gain -6.0
# Verify RMS decreased by ~6dB
```

#### Milestone 1.4: EQ Effect
```bash
# Success criteria
./nueva --input noise.wav --output filtered.wav --eq 1000:-12:1.0
# Spectral analysis shows dip at 1kHz
```

#### Milestone 1.5: Complete DSP Library
- Compressor, Reverb, Delay, Saturation, Gate, Limiter
- Each with test coverage

#### Milestone 1.6: DSP Chain + Serialization
```bash
# Success criteria
./nueva --input test.wav --chain config.json --output processed.wav
./nueva --project proj --save-state
./nueva --load-state proj/state.json --print-chain  # Shows same chain
```

### Phase 2: Layer Architecture (Week 2-3)

#### Milestone 2.1-2.4: Full Layer Implementation
- Layer 0 (immutable source)
- Layer 1 (AI state placeholder)
- Layer 2 (DSP chain)
- Bake operation

```bash
# Success criteria
./nueva --create-project proj --input audio.wav
./nueva --project proj --init-ai-layer
./nueva --project proj --gain -6.0
./nueva --project proj --bake
# Verify L0 is now the processed audio
```

### Phase 3: AI Bridge (Week 3-4)

#### Milestone 3.1: Python Bridge Protocol
```bash
# Success criteria
echo '{"action":"process","prompt":"test","model":"mock"}' | python -m nueva.ai_bridge
# Returns valid JSON response
```

#### Milestone 3.2: C++ ↔ Python Communication
```bash
# Success criteria
./nueva --project proj --ai-prompt "make it warmer" --ai-model mock --verbose
# Shows subprocess communication
```

#### Milestone 3.3: Mock AI Processor
- Keyword-based processing for testing
- Verifiable audio changes

### Phase 3.5: AI Agent (Week 4-5)

#### Milestone 3.5.1: Agent Reasoning Module
```bash
# Success criteria
python -m nueva.agent --test-prompt "make it punchier"
# Returns {"tool": "dsp", "effects": [...]}
```

#### Milestone 3.5.2: DSP Parameter Mapping
- Intent → specific parameters

#### Milestone 3.5.3: Conversational Refinement
- Multi-turn context
- Reference resolution

#### Milestone 3.5.4: Undo/Explain
- Full undo stack
- Explanation generation

### Phase 4: Real AI Models (Week 5-6)

#### Milestone 4.1: Style Transfer Integration
#### Milestone 4.2: Denoising Integration
#### Milestone 4.3: (Optional) ACE-Step Integration

### Phase 5: Polish & Edge Cases (Week 6-7)

- Error handling complete
- All edge cases covered
- Performance optimization
- Documentation

---

## 14. File Formats & Protocols <a name="formats"></a>

### 14.1 AI Bridge Protocol (Complete)

#### Request Schema
```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "required": ["action"],
  "properties": {
    "action": {
      "enum": ["ping", "process", "list_models", "get_model_info", "abort"]
    },
    "request_id": {"type": "string"},
    "prompt": {"type": "string"},
    "input_path": {"type": "string"},
    "output_path": {"type": "string"},
    "model": {"type": "string"},
    "model_params": {"type": "object"},
    "current_dsp_state": {"type": "object"},
    "available_tools": {
      "type": "array",
      "items": {"enum": ["dsp", "neural", "both"]}
    },
    "context": {
      "type": "object",
      "properties": {
        "conversation_history": {"type": "array"},
        "user_preferences": {"type": "object"},
        "audio_analysis": {"type": "object"}
      }
    }
  }
}
```

#### Response Schema
```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "required": ["success"],
  "properties": {
    "success": {"type": "boolean"},
    "request_id": {"type": "string"},
    "tool_used": {"enum": ["dsp", "neural", "both", "none"]},
    "reasoning": {"type": "string"},
    "message": {"type": "string"},
    "dsp_changes": {
      "type": "object",
      "properties": {
        "effects": {
          "type": "array",
          "items": {
            "type": "object",
            "properties": {
              "action": {"enum": ["add", "modify", "remove", "enable", "disable"]},
              "type": {"type": "string"},
              "id": {"type": "string"},
              "params": {"type": "object"}
            }
          }
        }
      }
    },
    "neural_changes": {
      "type": "object",
      "properties": {
        "model": {"type": "string"},
        "output_path": {"type": "string"},
        "processing_time_ms": {"type": "integer"}
      }
    },
    "ask_clarification": {"type": "boolean"},
    "clarification_question": {"type": "string"},
    "error": {"type": "string"},
    "error_code": {"type": "string"}
  }
}
```

### 14.2 Effect Serialization Format

```json
{
  "id": "eq_abc123",
  "type": "parametric_eq",
  "version": "1.0",
  "enabled": true,
  "params": {
    "bands": [
      {
        "index": 0,
        "frequency": 100,
        "gain_db": 3.0,
        "q": 0.7,
        "filter_type": "lowshelf",
        "enabled": true
      }
    ]
  },
  "metadata": {
    "added_at": "2024-01-15T12:00:00Z",
    "added_by": "agent",
    "add_reason": "User requested warmth, boosted low frequencies"
  }
}
```

---

## 15. Security Considerations <a name="security"></a>

### 15.1 Input Validation

```python
def validate_prompt(prompt: str) -> ValidationResult:
    """
    Validate user prompt before processing.
    """
    # Length limits
    if len(prompt) > 10000:
        return ValidationResult(valid=False, error="Prompt too long")
    
    # No file path injection in prompts
    dangerous_patterns = [
        r"\.\.\/",           # Path traversal
        r"[;&|`$]",          # Shell injection
        r"<script",          # XSS (shouldn't matter but defense in depth)
    ]
    for pattern in dangerous_patterns:
        if re.search(pattern, prompt, re.IGNORECASE):
            return ValidationResult(valid=False, error="Invalid characters in prompt")
    
    return ValidationResult(valid=True)


def validate_file_path(path: str, must_exist: bool = True) -> ValidationResult:
    """
    Validate file paths to prevent directory traversal.
    """
    # Resolve to absolute path
    resolved = Path(path).resolve()
    
    # Must be within allowed directories
    allowed_roots = [
        Path.home(),
        Path("/tmp"),
        # Add project directories
    ]
    
    if not any(is_subpath(resolved, root) for root in allowed_roots):
        return ValidationResult(valid=False, error="Path outside allowed directories")
    
    if must_exist and not resolved.exists():
        return ValidationResult(valid=False, error="File not found")
    
    return ValidationResult(valid=True, resolved_path=resolved)
```

### 15.2 Resource Limits

```python
RESOURCE_LIMITS = {
    "max_audio_duration_seconds": 7200,      # 2 hours
    "max_audio_file_size_bytes": 2_000_000_000,  # 2GB
    "max_project_size_bytes": 10_000_000_000,    # 10GB
    "max_undo_history": 50,
    "max_effects_in_chain": 20,
    "max_conversation_messages": 100,
    "max_concurrent_processes": 4,
    "processing_timeout_seconds": 600,        # 10 minutes
}
```

### 15.3 Sandboxing

For neural model execution:

```python
def run_model_sandboxed(model: str, input_path: str, output_path: str, params: dict):
    """
    Run neural model in isolated subprocess with resource limits.
    """
    import resource
    
    def set_limits():
        # Memory limit: 8GB
        resource.setrlimit(resource.RLIMIT_AS, (8 * 1024**3, 8 * 1024**3))
        # CPU time limit: 10 minutes
        resource.setrlimit(resource.RLIMIT_CPU, (600, 600))
    
    result = subprocess.run(
        ["python", "-m", f"nueva.models.{model}", input_path, output_path],
        preexec_fn=set_limits,
        timeout=RESOURCE_LIMITS["processing_timeout_seconds"],
        capture_output=True
    )
    
    return result
```

---

## 16. Performance Requirements <a name="performance"></a>

### 16.1 Target Benchmarks

| Operation | Target Time | Measurement |
|-----------|-------------|-------------|
| Project load | < 500ms | Time to ready state |
| DSP preview | < 50ms latency | Effect parameter change to audio out |
| Effect add/remove | < 100ms | UI response |
| Full export (3 min song) | < 10s | Render to file |
| Neural processing | < 30s | Model inference (excluding load) |
| Undo operation | < 100ms | State restoration |
| Agent response (DSP) | < 500ms | Prompt to action |
| Agent response (Neural) | < 2s + inference | Prompt to action start |

### 16.2 Memory Budget

| Component | Budget |
|-----------|--------|
| Audio buffer (3 min stereo) | ~150 MB |
| DSP chain state | < 10 MB |
| Undo history | < 500 MB |
| Model weights (if loaded) | 1-4 GB |
| Total working set | < 2 GB (without models) |

### 16.3 Optimization Strategies

```cpp
// Stream processing for large files
void processLargeFile(const File& input, const File& output, DSPChain& chain) {
    constexpr int BLOCK_SIZE = 8192;
    
    AudioFormatReader reader(input);
    AudioFormatWriter writer(output);
    
    AudioBuffer block(reader.numChannels, BLOCK_SIZE);
    
    while (reader.hasMoreSamples()) {
        reader.read(block, BLOCK_SIZE);
        chain.process(block);
        writer.write(block);
    }
}

// SIMD-optimized gain
void applyGain(float* data, int numSamples, float gain) {
    // Use JUCE's FloatVectorOperations for SIMD
    FloatVectorOperations::multiply(data, gain, numSamples);
}
```

---

## 17. AI Bridge Architecture (Daemon Mode) <a name="ai-bridge-daemon"></a>

**Cold Start Problem**: Loading neural model weights takes 5-30 seconds. If the Python bridge is a subprocess that dies after every command, you pay this penalty every time.

**Solution**: The AI Bridge runs as a persistent background daemon that keeps models warm.

### 17.1 Daemon Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         AI BRIDGE DAEMON                                     │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌─────────────┐     Unix Socket / TCP      ┌──────────────────────────┐   │
│  │   Nueva     │ ◄───────────────────────►  │    Python Daemon         │   │
│  │   (C++)     │      JSON Protocol         │                          │   │
│  └─────────────┘                            │  ┌────────────────────┐  │   │
│                                              │  │   Model Cache      │  │   │
│                                              │  │                    │  │   │
│  On startup:                                │  │  - ACE-Step (warm) │  │   │
│  1. C++ checks if daemon is running         │  │  - Denoise (warm)  │  │   │
│  2. If not, spawns it                       │  │  - Style (cold)    │  │   │
│  3. Daemon loads priority models            │  │  - Restore (cold)  │  │   │
│  4. Daemon stays alive between commands     │  │                    │  │   │
│                                              │  └────────────────────┘  │   │
│  On command:                                │                          │   │
│  1. C++ sends JSON via socket               │  ┌────────────────────┐  │   │
│  2. Daemon processes (model already loaded) │  │   LLM Agent        │  │   │
│  3. Daemon returns result                   │  │   (reasoning)      │  │   │
│  4. Daemon stays alive for next command     │  └────────────────────┘  │   │
│                                              │                          │   │
│  On idle (5 min):                           └──────────────────────────┘   │
│  - Unload least-used models to save VRAM                                   │
│  - Keep agent LLM loaded (small)                                            │
│                                                                              │
│  On shutdown / explicit kill:                                                │
│  - Graceful cleanup                                                          │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 17.2 Daemon Implementation

```python
# ai_bridge/daemon.py

import socket
import json
import threading
import time
from dataclasses import dataclass
from typing import Dict, Optional
import torch

SOCKET_PATH = "/tmp/nueva_ai_bridge.sock"  # Unix socket
TCP_PORT = 19847  # Fallback for Windows

@dataclass
class LoadedModel:
    model: any
    last_used: float
    load_time: float
    vram_mb: int

class AIBridgeDaemon:
    def __init__(self):
        self.models: Dict[str, LoadedModel] = {}
        self.agent = None  # LLM agent for reasoning
        self.lock = threading.Lock()
        self.running = True
        
        # Priority models to pre-load
        self.priority_models = ["denoise"]  # Small, frequently used
        
        # Model VRAM budgets (MB)
        self.model_vram = {
            "ace-step": 4000,
            "style-transfer": 2000,
            "denoise": 1000,
            "restore": 2000,
            "enhance": 3000
        }
        
        # VRAM budget (leave 2GB for system)
        self.vram_budget = self._detect_vram() - 2000
        
    def start(self):
        """Start the daemon server."""
        # Pre-load priority models
        for model_name in self.priority_models:
            self._load_model(model_name)
        
        # Initialize agent (always loaded - it's small)
        self._init_agent()
        
        # Start idle cleanup thread
        threading.Thread(target=self._idle_cleanup_loop, daemon=True).start()
        
        # Start socket server
        self._start_server()
    
    def _start_server(self):
        """Start Unix socket or TCP server."""
        if os.name == 'posix':
            # Unix socket (faster, more secure)
            if os.path.exists(SOCKET_PATH):
                os.remove(SOCKET_PATH)
            server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            server.bind(SOCKET_PATH)
        else:
            # TCP for Windows
            server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            server.bind(('127.0.0.1', TCP_PORT))
        
        server.listen(5)
        print(f"[DAEMON] Listening on {SOCKET_PATH if os.name == 'posix' else f'localhost:{TCP_PORT}'}")
        
        while self.running:
            client, _ = server.accept()
            threading.Thread(target=self._handle_client, args=(client,)).start()
    
    def _handle_client(self, client: socket.socket):
        """Handle a single request."""
        try:
            data = b""
            while True:
                chunk = client.recv(4096)
                if not chunk:
                    break
                data += chunk
                if b"\n" in data:
                    break
            
            request = json.loads(data.decode('utf-8'))
            response = self._process_request(request)
            client.sendall(json.dumps(response).encode('utf-8') + b"\n")
        except Exception as e:
            error_response = {"success": False, "error": str(e)}
            client.sendall(json.dumps(error_response).encode('utf-8') + b"\n")
        finally:
            client.close()
    
    def _process_request(self, request: dict) -> dict:
        """Process an agent request."""
        action = request.get("action")
        
        if action == "ping":
            return {"success": True, "message": "pong", "loaded_models": list(self.models.keys())}
        
        if action == "shutdown":
            self.running = False
            return {"success": True, "message": "shutting down"}
        
        if action == "process":
            return self._process_audio_request(request)
        
        if action == "preload":
            model = request.get("model")
            self._load_model(model)
            return {"success": True, "message": f"{model} loaded"}
        
        return {"success": False, "error": f"Unknown action: {action}"}
    
    def _process_audio_request(self, request: dict) -> dict:
        """Process an audio transformation request."""
        prompt = request.get("prompt")
        
        # Step 1: Agent decides what to do
        agent_decision = self.agent.decide(
            prompt=prompt,
            audio_analysis=request.get("audio_analysis"),
            current_dsp=request.get("current_dsp_state"),
            neural_context=request.get("neural_context")
        )
        
        # Step 2: If neural tool needed, ensure model is loaded
        if agent_decision.tool in ["neural", "both"]:
            model_name = agent_decision.neural_changes.get("model")
            if model_name not in self.models:
                load_start = time.time()
                self._load_model(model_name)
                agent_decision.metadata["model_load_time_ms"] = (time.time() - load_start) * 1000
            
            # Step 3: Run neural processing
            inference_start = time.time()
            self._run_neural_inference(
                model_name=model_name,
                input_path=request.get("input_path"),
                output_path=agent_decision.neural_changes.get("output_path"),
                params=agent_decision.neural_changes.get("params", {})
            )
            agent_decision.metadata["inference_time_ms"] = (time.time() - inference_start) * 1000
        
        return agent_decision.to_dict()
    
    def _load_model(self, model_name: str):
        """Load a model, evicting others if necessary."""
        with self.lock:
            if model_name in self.models:
                self.models[model_name].last_used = time.time()
                return
            
            # Check VRAM budget
            required = self.model_vram.get(model_name, 2000)
            current_usage = sum(m.vram_mb for m in self.models.values())
            
            while current_usage + required > self.vram_budget and self.models:
                # Evict least recently used
                lru_name = min(self.models.keys(), key=lambda k: self.models[k].last_used)
                print(f"[DAEMON] Evicting {lru_name} to free VRAM")
                del self.models[lru_name]
                torch.cuda.empty_cache()
                current_usage = sum(m.vram_mb for m in self.models.values())
            
            # Load model
            load_start = time.time()
            model = self._actually_load_model(model_name)
            load_time = time.time() - load_start
            
            self.models[model_name] = LoadedModel(
                model=model,
                last_used=time.time(),
                load_time=load_time,
                vram_mb=required
            )
            print(f"[DAEMON] Loaded {model_name} in {load_time:.2f}s")
    
    def _idle_cleanup_loop(self):
        """Periodically unload unused models."""
        IDLE_TIMEOUT = 300  # 5 minutes
        
        while self.running:
            time.sleep(60)
            
            with self.lock:
                now = time.time()
                to_evict = []
                
                for name, model in self.models.items():
                    if name not in self.priority_models:
                        if now - model.last_used > IDLE_TIMEOUT:
                            to_evict.append(name)
                
                for name in to_evict:
                    print(f"[DAEMON] Idle eviction: {name}")
                    del self.models[name]
                
                if to_evict:
                    torch.cuda.empty_cache()


# C++ side: Connect to daemon

class AIDaemonClient {
public:
    bool connect() {
        // Try Unix socket first
        #ifdef __unix__
        socket_fd = socket(AF_UNIX, SOCK_STREAM, 0);
        struct sockaddr_un addr;
        addr.sun_family = AF_UNIX;
        strcpy(addr.sun_path, "/tmp/nueva_ai_bridge.sock");
        
        if (::connect(socket_fd, (struct sockaddr*)&addr, sizeof(addr)) == 0) {
            return true;
        }
        #endif
        
        // Fall back to TCP
        socket_fd = socket(AF_INET, SOCK_STREAM, 0);
        struct sockaddr_in addr;
        addr.sin_family = AF_INET;
        addr.sin_port = htons(19847);
        inet_pton(AF_INET, "127.0.0.1", &addr.sin_addr);
        
        if (::connect(socket_fd, (struct sockaddr*)&addr, sizeof(addr)) == 0) {
            return true;
        }
        
        // Daemon not running - spawn it
        return spawnDaemon();
    }
    
    bool spawnDaemon() {
        // Start Python daemon as background process
        #ifdef __unix__
        pid_t pid = fork();
        if (pid == 0) {
            // Child process
            execl("/usr/bin/python", "python", "-m", "nueva.ai_bridge.daemon", nullptr);
            exit(1);
        }
        #else
        // Windows: CreateProcess
        STARTUPINFO si = { sizeof(si) };
        PROCESS_INFORMATION pi;
        CreateProcess(nullptr, "python -m nueva.ai_bridge.daemon", 
                      nullptr, nullptr, FALSE, CREATE_NO_WINDOW, 
                      nullptr, nullptr, &si, &pi);
        #endif
        
        // Wait for daemon to start
        for (int i = 0; i < 50; i++) {  // 5 second timeout
            std::this_thread::sleep_for(std::chrono::milliseconds(100));
            if (connect()) return true;
        }
        
        return false;
    }
    
    json sendRequest(const json& request) {
        std::string data = request.dump() + "\n";
        send(socket_fd, data.c_str(), data.size(), 0);
        
        std::string response;
        char buffer[4096];
        while (true) {
            int n = recv(socket_fd, buffer, sizeof(buffer) - 1, 0);
            if (n <= 0) break;
            buffer[n] = '\0';
            response += buffer;
            if (response.find('\n') != std::string::npos) break;
        }
        
        return json::parse(response);
    }
};
```

### 17.3 Latency Comparison

| Scenario | Subprocess Model | Daemon Model |
|----------|------------------|--------------|
| First DSP request | ~500ms (spawn Python) | ~100ms (socket connect) |
| Subsequent DSP requests | ~500ms each | ~50ms each |
| First Neural request | ~30s (spawn + load) | ~15s (load only, if cold) |
| Subsequent Neural (same model) | ~30s each | ~5s (model already warm) |
| Subsequent Neural (different model) | ~30s each | ~15s (load) or ~5s (if cached) |

---

## 18. Schema Versioning & Migration <a name="schema-versioning"></a>

**Problem**: What happens when you add new effect parameters in a future version?

### 18.1 Version Strategy

```json
{
  "schema_version": "1.0.0",
  "nueva_version": "0.1.0",
  "created_with_version": "0.1.0",
  "last_modified_with_version": "0.2.0",
  ...
}
```

**Versioning Rules**:
- `schema_version`: Follows semver
  - **MAJOR**: Breaking changes that require migration
  - **MINOR**: New optional fields (backward compatible)
  - **PATCH**: Documentation/clarification only
- Projects from newer versions are NOT guaranteed to work in older Nueva versions
- Projects from older versions MUST work in newer versions (with migration)

### 18.2 Migration System

```python
# migrations/registry.py

MIGRATIONS = {
    # (from_version, to_version): migration_function
    ("1.0.0", "1.1.0"): migrate_1_0_to_1_1,
    ("1.1.0", "1.2.0"): migrate_1_1_to_1_2,
    ("1.2.0", "2.0.0"): migrate_1_2_to_2_0,
}

def migrate_project(project_data: dict) -> dict:
    """
    Migrate project to current schema version.
    """
    current = project_data.get("schema_version", "1.0.0")
    target = CURRENT_SCHEMA_VERSION
    
    if current == target:
        return project_data
    
    # Find migration path
    path = find_migration_path(current, target)
    
    for from_ver, to_ver in path:
        migration_fn = MIGRATIONS[(from_ver, to_ver)]
        project_data = migration_fn(project_data)
        project_data["schema_version"] = to_ver
        log(f"Migrated {from_ver} → {to_ver}")
    
    return project_data


# Example migration: Adding new compressor parameter

def migrate_1_0_to_1_1(data: dict) -> dict:
    """
    v1.1.0 adds 'knee_db' parameter to compressor.
    Default to 0 (hard knee) for existing projects.
    """
    for effect in data.get("layer2", {}).get("chain", []):
        if effect["type"] == "compressor":
            if "knee_db" not in effect["params"]:
                effect["params"]["knee_db"] = 0.0  # Hard knee default
    
    return data


def migrate_1_1_to_1_2(data: dict) -> dict:
    """
    v1.2.0 adds 'neural_blend' configuration.
    Default to disabled for existing projects.
    """
    if "neural_blend" not in data:
        data["neural_blend"] = {
            "enabled": False,
            "blend": 1.0
        }
    
    return data


def migrate_1_2_to_2_0(data: dict) -> dict:
    """
    v2.0.0 restructures layer storage.
    Breaking change - old 'layer1_ai.wav' path moves to 'audio/layer1/'.
    """
    # This is a major migration - backup first
    old_layer1_path = data.get("layer1", {}).get("path")
    if old_layer1_path:
        new_path = f"audio/layer1/{os.path.basename(old_layer1_path)}"
        data["layer1"]["path"] = new_path
        # Actual file move handled by project loader
        data["_pending_file_moves"] = [
            {"from": old_layer1_path, "to": new_path}
        ]
    
    return data
```

### 18.3 Unknown Fields Policy

```python
def load_project(path: str) -> Project:
    data = json.load(open(path / "project.json"))
    
    # Migrate if needed
    data = migrate_project(data)
    
    # Parse known fields, PRESERVE unknown fields
    project = Project()
    project.schema_version = data["schema_version"]
    project.layer0 = Layer0.from_dict(data["layer0"])
    project.layer1 = Layer1.from_dict(data["layer1"])
    project.layer2 = Layer2.from_dict(data["layer2"])
    
    # Store unknown fields for round-trip preservation
    known_keys = {"schema_version", "layer0", "layer1", "layer2", ...}
    project._unknown_fields = {k: v for k, v in data.items() if k not in known_keys}
    
    return project


def save_project(project: Project, path: str):
    data = {
        "schema_version": CURRENT_SCHEMA_VERSION,
        "nueva_version": NUEVA_VERSION,
        "layer0": project.layer0.to_dict(),
        "layer1": project.layer1.to_dict(),
        "layer2": project.layer2.to_dict(),
        # Preserve unknown fields from newer versions
        **project._unknown_fields
    }
    
    json.dump(data, open(path / "project.json", "w"), indent=2)
```

---

## 19. Storage Management <a name="storage-management"></a>

### 19.1 Layer 1 Pruning

**Problem**: If user prompts AI 20 times, the project folder fills with multi-GB audio files.

```python
class Layer1StorageManager:
    """
    Manages Layer 1 audio file lifecycle.
    Prunes unreachable files when undo history is trimmed.
    """
    
    def __init__(self, project_path: Path):
        self.project_path = project_path
        self.audio_dir = project_path / "audio" / "layer1"
        
    def record_new_layer1(self, audio_path: Path, undo_action_id: str):
        """
        Record that this Layer 1 file is associated with an undo action.
        """
        manifest = self._load_manifest()
        manifest["files"][audio_path.name] = {
            "created_at": datetime.now().isoformat(),
            "undo_action_id": undo_action_id,
            "size_bytes": audio_path.stat().st_size
        }
        self._save_manifest(manifest)
    
    def prune_orphaned_files(self, reachable_action_ids: Set[str]):
        """
        Delete Layer 1 files that are no longer reachable via undo history.
        Called when undo stack is trimmed or project is saved.
        """
        manifest = self._load_manifest()
        current_layer1 = self._get_current_layer1_filename()
        
        to_delete = []
        for filename, info in manifest["files"].items():
            action_id = info.get("undo_action_id")
            
            # Keep if: current file, or reachable via undo
            if filename == current_layer1:
                continue
            if action_id in reachable_action_ids:
                continue
            
            to_delete.append(filename)
        
        # Delete orphaned files
        total_freed = 0
        for filename in to_delete:
            file_path = self.audio_dir / filename
            if file_path.exists():
                size = file_path.stat().st_size
                file_path.unlink()
                total_freed += size
                del manifest["files"][filename]
                log(f"Pruned orphaned Layer 1: {filename} ({size / 1e6:.1f} MB)")
        
        self._save_manifest(manifest)
        return total_freed
    
    def get_storage_usage(self) -> dict:
        """
        Report storage usage for Layer 1 files.
        """
        manifest = self._load_manifest()
        total_size = sum(f["size_bytes"] for f in manifest["files"].values())
        
        return {
            "file_count": len(manifest["files"]),
            "total_size_bytes": total_size,
            "total_size_mb": total_size / 1e6
        }


# In UndoManager, when trimming history:

class UndoManager:
    def _trim_history(self):
        """Remove oldest undo entries when over limit."""
        while len(self.undo_stack) > self.max_undo_levels:
            removed = self.undo_stack.pop(0)
            self._discarded_action_ids.add(removed.action_id)
        
        # Prune orphaned Layer 1 files
        reachable = {a.action_id for a in self.undo_stack + self.redo_stack}
        self.storage_manager.prune_orphaned_files(reachable)
```

### 19.2 Storage Warnings

```python
def check_storage_health(project: Project) -> List[str]:
    """
    Check for storage issues and return warnings.
    """
    warnings = []
    
    # Layer 1 bloat
    l1_usage = project.layer1_storage.get_storage_usage()
    if l1_usage["total_size_mb"] > 1000:  # 1GB
        warnings.append(
            f"⚠️ Layer 1 storage is using {l1_usage['total_size_mb']:.0f} MB "
            f"({l1_usage['file_count']} files). Consider pruning history."
        )
    
    # Disk space
    free_space = shutil.disk_usage(project.path).free
    if free_space < 1e9:  # 1GB
        warnings.append(
            f"⚠️ Low disk space: {free_space / 1e9:.1f} GB remaining"
        )
    
    # Backup size
    backup_size = sum(f.stat().st_size for f in (project.path / "backups").glob("*"))
    if backup_size > 2e9:  # 2GB
        warnings.append(
            f"⚠️ Backups using {backup_size / 1e9:.1f} GB. Consider cleaning old backups."
        )
    
    return warnings
```

---

## 20. Stereo/Mono & Phase Handling <a name="stereo-phase"></a>

### 20.1 Channel Configuration

```cpp
enum class ChannelConfig {
    MONO,           // 1 channel
    STEREO,         // 2 channels (L, R)
    MID_SIDE,       // 2 channels (M, S) - for certain processing
    // Future: SURROUND_5_1, etc.
};

class ChannelManager {
public:
    // Convert mono to stereo (duplicate channel)
    static AudioBuffer monoToStereo(const AudioBuffer& mono) {
        AudioBuffer stereo(2, mono.getNumSamples());
        stereo.copyFrom(0, 0, mono, 0, 0, mono.getNumSamples());
        stereo.copyFrom(1, 0, mono, 0, 0, mono.getNumSamples());
        return stereo;
    }
    
    // Convert stereo to mono (sum and normalize)
    static AudioBuffer stereoToMono(const AudioBuffer& stereo) {
        AudioBuffer mono(1, stereo.getNumSamples());
        mono.copyFrom(0, 0, stereo, 0, 0, stereo.getNumSamples());
        mono.addFrom(0, 0, stereo, 1, 0, stereo.getNumSamples());
        mono.applyGain(0.5f);  // Normalize
        return mono;
    }
    
    // Check for phase issues
    static float calculateCorrelation(const AudioBuffer& stereo) {
        if (stereo.getNumChannels() < 2) return 1.0f;  // Mono = perfect correlation
        
        const float* L = stereo.getReadPointer(0);
        const float* R = stereo.getReadPointer(1);
        int n = stereo.getNumSamples();
        
        float sumL = 0, sumR = 0, sumLR = 0, sumL2 = 0, sumR2 = 0;
        for (int i = 0; i < n; i++) {
            sumL += L[i];
            sumR += R[i];
            sumLR += L[i] * R[i];
            sumL2 += L[i] * L[i];
            sumR2 += R[i] * R[i];
        }
        
        float meanL = sumL / n, meanR = sumR / n;
        float num = sumLR / n - meanL * meanR;
        float den = std::sqrt((sumL2 / n - meanL * meanL) * (sumR2 / n - meanR * meanR));
        
        return den > 0 ? num / den : 1.0f;
    }
};
```

### 20.2 DSP Chain Channel Handling

```cpp
class DSPChain {
    void process(AudioBuffer& buffer) {
        ChannelConfig inputConfig = detectConfig(buffer);
        
        for (auto& effect : effects) {
            ChannelConfig effectConfig = effect->getOutputConfig(inputConfig);
            
            // Handle mono → stereo expansion (e.g., reverb on mono input)
            if (inputConfig == ChannelConfig::MONO && 
                effectConfig == ChannelConfig::STEREO) {
                buffer = ChannelManager::monoToStereo(buffer);
            }
            
            // Process
            effect->process(buffer);
            
            // Check for phase issues after stereo effects
            if (effect->canCausePhaseIssues()) {
                float corr = ChannelManager::calculateCorrelation(buffer);
                if (corr < 0.3f) {
                    logWarning("Phase correlation dropped to %.2f after %s", 
                               corr, effect->getName().c_str());
                }
            }
            
            inputConfig = effectConfig;
        }
    }
};

// Effect declarations
class ReverbEffect : public Effect {
    ChannelConfig getOutputConfig(ChannelConfig input) override {
        return ChannelConfig::STEREO;  // Reverb always outputs stereo
    }
    
    bool canCausePhaseIssues() override { return true; }
};

class CompressorEffect : public Effect {
    ChannelConfig getOutputConfig(ChannelConfig input) override {
        return input;  // Preserves channel count
    }
    
    bool canCausePhaseIssues() override { return false; }
};
```

### 20.3 Phase Awareness in Agent

```python
# In agent system prompt:

"""
## Phase Awareness

When adding stereo effects, watch for phase cancellation:

- Reverb: Can cause phase issues with pre-delay and stereo width
- Delay: Ping-pong delay can cause comb filtering
- Stereo width enhancement: High amounts can cause mono compatibility issues

Before adding stereo-widening effects, check the audio analysis:
- If `stereo_correlation` < 0.5: Source already has phase concerns
- If `stereo_correlation` > 0.95: Source is nearly mono, widening may help

After adding stereo effects, warn user if correlation drops below 0.3.

## Mono/Stereo Transitions

- Mono input + Reverb → Stereo output (this is fine)
- Stereo input + Mono reference (style transfer) → May collapse stereo image
- When Neural tool uses mono model on stereo input, offer to process M/S separately
"""
```

### 20.4 Agent "Do No Harm" Enhancement

```python
# Addition to agent system prompt:

"""
## Do No Harm Rule

Before applying any change, verify it won't damage the audio:

1. **Clipping Prevention**: If your changes would push peaks above 0dBFS:
   - Automatically insert a Limiter at -1dB ceiling
   - Inform user: "I added a limiter to prevent clipping"
   - This dialog auto-dismisses after 3 seconds

2. **Phase Protection**: If stereo correlation would drop below 0.2:
   - Warn user before applying
   - Suggest reducing effect intensity

3. **Loudness Sanity**: If LUFS would exceed -5 (extremely loud):
   - Warn user this exceeds all streaming standards
   - Suggest more moderate settings

4. **Don't Remove Intentional Artifacts**: Check Neural Context before using:
   - Gates/noise reduction after vinyl preset
   - EQ corrections after style transfer
   - Declipping after saturation

5. **Preserve User Work**: When user has manual DSP tweaks:
   - Default to PRESERVING Layer 2 on Neural re-invoke
   - Only reset if explicitly requested
"""
```

---

## Appendix A: Complete CLI Reference

```
Nueva Audio Processor v1.0

USAGE:
    nueva [OPTIONS] [COMMAND]

COMMANDS:
    (none)                    Interactive mode
    --help, -h                Show this help
    --version                 Show version

PROJECT MANAGEMENT:
    --create-project <path>   Create new project directory
    --project <path>          Load existing project
    --save-state              Save current state
    --load-state <file>       Load state from file

AUDIO I/O:
    --input <file>            Input audio file
    --output <file>           Output audio file
    --export <file>           Export final mix
    --generate-test-tone      Generate test tone
      --freq <hz>               Frequency (default: 440)
      --duration <sec>          Duration (default: 2)

DSP EFFECTS:
    --gain <dB>               Apply gain
    --eq <freq>:<gain>:<q>    Add EQ band
    --compress <threshold>:<ratio>:<attack>:<release>
    --reverb <size>:<damping>:<wet>
    --delay <time>:<feedback>:<wet>
    --chain <file>            Load effect chain from JSON

AI PROCESSING:
    --ai-prompt <text>        Process with AI agent
    --ai-model <name>         Specify neural model
    --ai-reference <file>     Reference audio for style transfer

LAYER OPERATIONS:
    --init-ai-layer           Initialize Layer 1
    --reset-ai                Reset Layer 1 to Layer 0
    --reset-dsp               Clear DSP chain
    --bake                    Flatten all layers

UNDO/REDO:
    --undo                    Undo last action
    --redo                    Redo undone action
    --history                 Show action history

DEBUG:
    --verbose                 Verbose output
    --print-chain             Print current DSP chain
    --print-state             Print project state
    --analyze <file>          Analyze audio file
```

---

## Appendix B: Agent System Prompt (Complete)

```
You are an expert audio engineer assistant integrated into Nueva, a digital audio workstation. Your role is to help users achieve their sonic goals through a combination of traditional DSP effects and AI-powered audio processing.

## Your Capabilities

### DSP Tool (Layer 2 - Instant, Non-destructive)
You can add, modify, or remove these effects:
- **Parametric EQ**: Shape frequency response with up to 8 bands (20Hz-20kHz, ±24dB, Q 0.1-10)
- **Compressor**: Control dynamics (threshold -60 to 0dB, ratio 1:1 to 20:1, attack 0.1-100ms, release 10-1000ms)
- **Reverb**: Add space (room size 0-1, damping 0-1, wet/dry 0-1, pre-delay 0-100ms)
- **Delay**: Add echoes (1-2000ms, feedback 0-95%, ping-pong option)
- **Saturation**: Add warmth/harmonics (tape, tube, transistor types, drive 0-1)
- **Gate**: Remove noise (threshold -80 to 0dB, attack 0.1-50ms, release 10-500ms)
- **Limiter**: Prevent clipping (ceiling -12 to 0dB, true peak detection)
- **Gain**: Simple volume adjustment (-96 to +24dB)

DSP changes are instant and fully adjustable afterward.

### Neural Tool (Layer 1 - Regenerates Audio)
You can invoke AI models for:
- **ace-step**: Full music transformation (cover, repaint, style change, genre shift)
- **style-transfer**: Make audio "sound like" a reference or preset (vintage, lo-fi, tape, etc.)
- **denoise**: AI-powered noise/hiss/hum removal
- **restore**: Fix clipped, degraded, or damaged audio (declip, declick, decrackle)
- **enhance**: AI upsampling and clarity/presence enhancement

Neural processing takes 5-30 seconds and regenerates the audio. The user can then apply DSP on top.

## Critical Rules

### 1. DO NO HARM
Before applying any change, verify it won't damage the audio:

**Clipping Prevention**: If your changes would push peaks above 0dBFS:
- Automatically insert a Limiter at -1dB ceiling at the end of the chain
- Tell the user: "I added a limiter to prevent clipping"

**Phase Protection**: If a stereo effect would drop correlation below 0.2:
- Warn user before applying
- Suggest reducing effect intensity

**Loudness Sanity**: If LUFS would exceed -5:
- Warn user this exceeds all streaming standards

### 2. NEURAL CONTEXT AWARENESS
After Neural processing, check what was intentionally added:

{neural_context_warnings}

DO NOT use DSP to "fix" intentional artifacts from Neural processing:
- Don't gate after vinyl preset (crackle is intentional)
- Don't EQ-correct after style transfer (new timbre is intentional)
- Don't declip after saturation (harmonics are intentional)

### 3. PRESERVE USER WORK
When Neural tool regenerates Layer 1:
- DEFAULT: Preserve Layer 2 DSP chain (keep user's manual tweaks)
- Only reset Layer 2 if user explicitly says "start fresh" or "redo everything"

### 4. ANALYZE BEFORE ACTING
Current audio characteristics:
{audio_summary}

Use this analysis to inform your decisions:
- If clipping detected → Suggest restore/declip before other processing
- If very loud (>-9 LUFS) → Be cautious with gain, likely already limited
- If quiet (<-20 LUFS) → May need gain/limiting
- If noisy (>-50dB floor) → Consider denoise
- If phase issues → Be cautious with stereo widening

## Decision Guidelines

1. **Prefer DSP** for standard mixing tasks - it's faster and more controllable
2. **Use Neural** for holistic transformations that can't be achieved with parameters
3. **Use Both** when request has DSP-appropriate AND neural-appropriate components
4. **Ask for clarification** if the request is ambiguous (confidence < 50%)
5. **Explain your reasoning** briefly after making changes
6. **Use professional terminology** but remain accessible

## Phrase → Action Mapping

| User Says | Tool | Action |
|-----------|------|--------|
| "warmer" | DSP | Low shelf +2dB @ 200Hz, high shelf -2dB @ 8kHz |
| "brighter" | DSP | High shelf +3dB @ 8kHz |
| "punchier" | DSP | Compressor (fast attack 5ms, 4:1) + EQ boost @ 100-150Hz |
| "cleaner" | ASK | Could mean denoise (Neural) OR cut mud (DSP) |
| "more presence" | DSP | EQ peak +3dB @ 2-4kHz |
| "more air" | DSP | High shelf +2dB @ 12kHz |
| "vintage" / "60s" / "old" | Neural | style-transfer with appropriate preset |
| "remove noise" / "hiss" | Neural | denoise model |
| "fix clipping" | Neural | restore (declip mode) |
| "like a [genre]" | Neural | ace-step or style-transfer |
| "reimagine as" | Neural | ace-step (transform mode) |
| "add reverb" | DSP | Reverb with medium room preset |
| "compress it" | DSP | Compressor with gentle preset |
| "make it louder" | DSP | Gain +3-6dB (check headroom first!) |

## Conversation Context

- Remember what effects you've added in this session
- "That" or "it" refers to the most recently discussed effect
- "More" or "less" means modify existing, not add new
- "Undo" means revert your last change
- "What did you do?" triggers explanation mode
- "Actually..." means undo and reinterpret

## Effect Chain Order

When adding effects, place them in this order (unless user specifies otherwise):
1. Gate (clean up noise first)
2. EQ (corrective - remove problems)
3. Compressor
4. EQ (creative - add color)
5. Saturation
6. Delay
7. Reverb
8. Limiter (always last)

## Stereo/Mono Awareness

- Mono input + Reverb → Outputs stereo (this is fine)
- Check `stereo_correlation` before adding widening effects
- If correlation < 0.5, source already has phase concerns - be cautious
- After stereo effects, warn if correlation drops below 0.3

## Current Project State
{project_state_json}

## Audio Analysis
{audio_analysis_json}

## Neural Context Warnings
{neural_context_warnings}

## Conversation History
{conversation_history}

## User Request
{user_prompt}

Respond with a JSON object:
{
  "tool": "dsp" | "neural" | "both" | "none",
  "reasoning": "Brief explanation of your decision",
  "confidence": 0.0-1.0,
  "dsp_changes": [
    {
      "action": "add" | "modify" | "remove" | "enable" | "disable",
      "type": "effect_type",
      "id": "existing_effect_id (for modify/remove)",
      "params": {...},
      "position": "optional chain position"
    }
  ],
  "neural_changes": {
    "model": "model_name",
    "params": {...},
    "preserve_layer2": true
  },
  "safety_actions": [
    {"action": "add_limiter", "reason": "prevent clipping"}
  ],
  "ask_clarification": false,
  "clarification_question": null,
  "message": "Friendly response to user explaining what you did"
}
```

---

**END OF SPECIFICATION**

This document contains everything needed to implement Nueva. Follow the phases in order, verify each milestone before proceeding, and refer to the edge cases and error handling sections when encountering unexpected situations.

---

## Appendix C: Implementation Priority Notes for Claude Code

### Phase Priority
1. **Phase 1 (Audio Engine)**: CRITICAL - If `--chain config.json` doesn't work perfectly, the AI agent has nothing to control
2. **Phase 3.3 (Mock AI)**: Implement BEFORE real PyTorch models - the pipeline logic matters more than inference quality for v1
3. **Phase 17 (Daemon)**: Can start with subprocess model, migrate to daemon after core works

### Testing Priority
- Verification without listening (RMS, FFT, crest factor) enables CI/CD
- Test edge cases: silent audio, clipped audio, mono→stereo, very long files

### Known JUCE Gotchas
- `AudioFormatManager` needs formats registered before use
- `AudioBuffer` is 0-indexed for channels
- Remember to call `prepareToPlay()` on DSP processors before use
- Watch for sample rate mismatches between files

---

## Appendix D: Future Extensions (Out of Scope for v1)

These are all valuable but should wait until the core CLI workflow is solid and tested:

- GUI (JUCE has excellent UI components, but functionality first)
- Real-time preview (requires careful threading)
- VST/AU plugin hosting (process through third-party effects)
- MIDI integration
- Multi-track/arrangement view
- Cloud model inference
- Collaborative editing
