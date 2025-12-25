# Phase 2 Development Phases

## Development Phases

### Phase 2A — Agent Interaction Model

| Task | Status |
|-----|--------|
| Define agent roles | Complete |
| Agent identity model | Complete |
| Writer append rules | Complete |
| Reader access rules | Complete |
| Concurrent access safety | Complete |
| Agent authorization checks | Complete |

**Exit Criteria:**
- Agent roles clearly defined (Reader, Writer, Synthesis)
- Authorization enforced on all write operations
- Concurrent agents can operate safely
- Agent identity preserved in all frames

---

### Phase 2B — Core Context APIs

| Task | Status |
|-----|--------|
| GetNode API | Complete |
| PutFrame API | Complete |
| ContextView wiring | Complete |
| Error model | Complete |
| API determinism tests | Complete |
| Concurrent request handling | Complete |

**Exit Criteria:**
- ✅ GetNode and PutFrame APIs implemented
- ✅ APIs are deterministic (same inputs → same outputs)
- ✅ Error handling is comprehensive and deterministic
- ✅ Concurrent requests handled safely

---

### Phase 2C — Branch Synthesis

| Task | Status |
|-----|--------|
| Synthesis frame types | Complete |
| Bottom-up traversal logic | Complete |
| Basis construction rules | Complete |
| Synthesis triggers | Complete |
| Synthesis policies | Complete |
| Determinism tests | Complete |

**Exit Criteria:**
- ✅ Branch synthesis algorithm implemented
- ✅ Synthesis is deterministic (same inputs → same outputs)
- ✅ Bottom-up synthesis enforced
- ✅ Multiple synthesis policies supported

---

### Phase 2D — Incremental Regeneration

| Task | Status |
|-----|--------|
| Basis diff detection | Todo |
| Regeneration workflow | Todo |
| Atomic head updates | Todo |
| Basis index implementation | Todo |
| Regeneration tests | Todo |
| Idempotency tests | Todo |

**Exit Criteria:**
- Basis change detection working
- Regeneration only affects changed frames
- Regeneration is idempotent
- Old frames preserved (append-only)

---

### Phase 2E — Multi-Frame Composition

| Task | Status |
|-----|--------|
| Composition policies | Todo |
| Ordering strategies | Todo |
| Bounded output enforcement | Todo |
| Multi-source composition | Todo |
| Determinism tests | Todo |

**Exit Criteria:**
- Composition policies implemented
- Composition is deterministic
- Output is bounded (max frames enforced)
- Multiple composition sources supported

---

### Phase 2F — Model Provider Integration

| Task | Status |
|-----|--------|
| Model provider abstraction trait | Todo |
| OpenAI provider implementation | Todo |
| Anthropic provider implementation | Todo |
| Ollama provider implementation | Todo |
| Custom local provider support | Todo |
| Provider error handling | Todo |
| Agent-provider integration | Todo |
| Provider configuration | Todo |
| Streaming support | Todo |
| Provider tests | Todo |

**Exit Criteria:**
- Multiple providers supported (OpenAI, Anthropic, Ollama, custom local)
- Unified API across all providers
- Provider errors mapped to ApiError
- Agents can specify and switch providers
- Local providers work with OpenAI-compatible format
- Streaming support implemented
- Provider configuration validated

---

### Phase 2G — Tooling & Integrations

| Task | Status |
|-----|--------|
| CLI tooling | Todo |
| Editor integration hooks | Todo |
| CI integration | Todo |
| Internal agent adapters | Todo |
| Tool idempotency tests | Todo |

**Exit Criteria:**
- CLI commands implemented
- Tools are idempotent
- Editor hooks functional
- CI integration working
- Clear separation from core engine

---

## Phase Exit Criteria

Phase 2 is complete when:
- Agents can reliably read and write context
- Branch context is synthesized incrementally
- Regeneration is minimal and deterministic
- Workflows compose without search or mutation
- Multiple LLM providers supported with unified API
- Agents can use cloud or local providers seamlessly
- All components tested and documented
- Tooling is functional and idempotent

---

[← Back to Phase 2 Spec](phase2_spec.md)
