# Handoff Prompt — FigCraft AI Port (Zenuxs → FigCraft)

> Ye document doosre agent ko dena hai. Copy karo aur us agent ko send karo.
> Hindi/Hinglish instructions hain, technical terms English me.

---

## 1. TASK KYA THA (background)

User ka previous project `/home/azaz/Projects/zenuxs-desigen` (TypeScript/Vue, "OpenPencil") tha.
Uska **poora AI subsystem** pure-Rust (egui) app `/home/azaz/reference-outside-workspace/figcraft`
me port karna tha: AI chat, providers/models/custom provider, skills, plugins, usages,
MCP + automation, media, accounts & cloud, cloud storage, aur AI & Agents settings.

**Status: poora port ho chuka hai, 21 M13 commits, working tree clean.**

---

## 2. KYA KYA ADD KIYA (complete inventory)

### Naye crates
- **`crates/ai`** (L2, pure, no network): provider registry, model connections/profiles/roles,
  credential store, skills registry + router, usage, transport/mcp/media/cloud/storage traits,
  plus pure rules for vision + image sourcing (`vision.rs`, `assets.rs`).
- **`crates/ai-net`** (L6, network): SSE reader, OpenAI chat-completions + Responses, Anthropic,
  Google, MCP streamable-HTTP client, images (generate/stock/download/vectorize/remove-bg),
  cloud client, S3 SigV4 client, ACP external-agent transport.

### Engine (`crates/engine`)
- `ai.rs` / `cmd/ai.rs` — ~59 `ai.*` commands (settings, keys, chat, cloud, storage, diagnostics)
- `chat.rs` / `chat_tools.rs` — conversation state + 17 tools (model ke "hands")
- Settings windows, right-panel AI tab, worker-thread runtime

### UI (`crates/ui-egui`)
- **AI & Agents settings** — 10 tabs: General, Models, Skills, Media, Plugins, Storage,
  Automation, Account, Usage, Diagnostics
- **Right panel "AI" tab** — streaming chat, reasoning, tool calls, history

### Zenuxs se port kiye gaye features (kya karte hain)

| Feature | Kya karta hai |
|---|---|
| AI chat bot | Streaming model conversation; reasoning; tool loop; repair rounds |
| Providers/models | 9 providers + custom OpenAI/Anthropic-compatible base URL + model ID |
| Skills | 30 semantic skills + router + user-written standing instructions |
| Plugins | Curated directory (categories, permissions, runnable/unavailable) |
| Usages | Token ledger + Usage panel |
| MCP + automation | External streamable-HTTP MCP servers; tools `mcp__server__tool` prefixed |
| Media | generate_image, stock_photo, source_image (no-key), vectorize, remove_background |
| No-key image sourcing | Openverse + Wikimedia → Pexels (key) → generate, lazy fallback + licence/SSRF rules |
| Accounts & cloud | Argon2 signup/login, sessions, lockout, per-user documents |
| Cloud storage | S3-compatible (SigV4) + server provider, sync outbox, open-from-cloud |
| ACP agents | Claude Code / Codex / Gemini CLI ke through chat |
| Chat history | Saved conversations with the preferences |

---

## 3. LIVE TEST KAISE KAREIN (already proven working)

### Environment
- Local OpenAI-compatible proxy: `http://127.0.0.1:20129/v1`, key `public`
  (check: `curl http://127.0.0.1:20129/v1/models`)
- Desktop binary: `/home/azaz/reference-outside-workspace/figcraft/target/debug/figcraft`

### Chalane ke steps
```sh
cd /home/azaz/reference-outside-workspace/figcraft
DISPLAY=:0 PATH="/home/azaz/.local/bin:/home/azaz/toolchain/usr/bin:$PATH" \
  setsid ./target/debug/figcraft --control 59253 > /tmp/fc.log 2>&1 < /dev/null & disown
sleep 8
python3 docs/ai_smoke.py          # 20+ checks, ✅/❌ print karta hai
```

### Jo pehle chal ke proof hua (manual e2e)
Custom `openai-compatible` connection → model `mimo-v2.6-flash-free` → role `design` →
credential store. Chat prompt: *"Make a 300x120 rectangle named Banner with red fill"*.
Model ne tool sequence chalaya: `node.create` (galat fills) → repair → `node.set` →
canvas pe red Banner layer bana. Screenshot: `/tmp/opencode/ai_design2.png`.

### Ready-made test files
- `docs/ai_smoke.py` — comprehensive control-channel test (sab features)
- `docs/ai-port-smoke-test.md` — short usage guide

---

## 4. QUEUE KIYE GAYE BUG FIXES (committed)

- `53896bb1` — **duplicate chat message fix**: programmatic sends (`ai.chat.send`, actions,
  control, MCP) message do baar append karte the. `send()` vs `send_pending()` alag kiya.
  Regression test: `a_queued_message_is_not_appended_twice`.

---

## 5. VERIFICATION STATUS (gate results)

- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `xtask layers | docs | assets | wasm` — all ok
- i18n tests — 28 passed
- Full workspace tests — **sirf 3 pre-existing failures**: `board_mindmap_tests` (clean `main` pe
  bhi fail hain, stash karke verify hua). Baaki sab pass.

---

## 6. KYA NAHI HUA / LIMITATIONS (important)

1. **Web build me AI nahi** — `BrowserTransport::available() → false`. Web build compile aur serve
   hota hai (`trunk serve`, port 8099, HTTP 200, 26.5 MB WASM), lekin browser me AI chat kaam
   nahi karega. Live AI sirf desktop app me. (Async fetch redesign chahiye — bada task.)
2. **Live image sourcing / image tools** — rules + providers unit-tested hain, lekin real
   Openverse/Wikimedia endpoints ke against network call nahi chalaya gaya (browser nahi hai;
   proxy sirf chat/modality ke liye hai).
3. **Koi real API key repo me nahi** — key env var ya local config me dete hain.

---

## 7. DOOSRE AGENT KE LIYE TEKNISSH NOTES

- Build ke liye: `export PATH="/home/azaz/.local/bin:/home/azaz/toolchain/usr/bin:$PATH"` aur
  `export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc-15` har cargo command se pehle.
- Control channel newline-delimited JSON: `{"id":1,"method":"engine.execute","params":{...}}`.
- `ui.screenshot` tabhi kaam karta hai jab X window visible ho; "covered" error aaye to
  `window.focus {"id":0}` ya Xvfb reset karo.
- `docs/AGENTS.md` house rules follow karo: no commit to main without PR (yahan local commits chal rahe hain),
  clean-room, no secrets in repo, assets rules.
- Commits milestone id ke saath: `M13: ...`

---

## 8. HANDOFF PROMPT (doosre agent ko exactly ye bhejo)

```
Tum ek existing project pe continue kar rahe ho: /home/azaz/reference-outside-workspace/figcraft
(pure-Rust egui app). Pehle README/AGENTS.md aur docs/gaps.md (gap 19) padho.

User ka previous project /home/azaz/Projects/zenuxs-desigen ka AI subsystem isme poora port ho
chuka hai (21 M13 commits, `git log --oneline --grep=M13` dekho). Naye crates: crates/ai (pure
rules/types) aur crates/ai-net (network). Features: AI chat + tool loop, 9 providers + custom,
30 skills, plugins directory, usage, MCP client, media tools (no-key image sourcing via
Openverse+Wikimedia→Pexels→generate, vectorize, remove_background), accounts+cloud storage,
ACP agents, saved chat history, 10-tab AI & Agents settings.

TUMHARA KAM:
1. Poora end-to-end test karo — `docs/ai_smoke.py` Chalao (app ko `--control 59253` se start karke).
   Local OpenAI-compatible proxy available hai: http://127.0.0.1:20129/v1, key "public",
   model "mimo-v2.6-flash-free". Har feature ka ✅/❌ record karo.
2. Jo ❌ aayein unko debug karke fix karo (regression test ke saath), narrow diff rakho.
3. Gate chalao: cargo clippy -D warnings, xtask layers/docs/assets/wasm, i18n, full tests.
   3 pre-existing board_mindmap failures ignore karo (clean main pe bhi fail).
4. Working tree clean rakho; commits `M13: ...` milestone ke saath.

Constraints:
- Keys/repo secrets kabhi commit ya log me nahi. Build env: PATH me /home/azaz/.local/bin aur
  /home/azaz/toolchain/usr/bin, CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER set karo.
- Web build me AI abhi nahi hai (BrowserTransport::available=false) — ye known limitation hai,
  fix karne ki zarurat nahi jab tak bola na jaye.
- Visual screenshot ke liye X window visible honi chahiye.

Pehle `git status`, `git log -5`, aur `docs/ai_smoke.py` padho, phir test shuru karo.
```
