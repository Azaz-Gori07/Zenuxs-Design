# AI port — end-to-end smoke test

Ye script Zenuxs se port kiye gaye **har AI feature** ko aapke local OpenAI-compatible
proxy (`http://127.0.0.1:20129/v1`, key `public`) ke against exercise karta hai.

## Chalane se pehle
1. Desktop app chalao:

   ```sh
   cd /home/azaz/reference-outside-workspace/figcraft
   DISPLAY=:0 ./target/debug/figcraft --control 59253
   ```

2. Proxy chal raha hona chahiye (`curl http://127.0.0.1:20129/v1/models`).

3. Test chalao:

   ```sh
   python3 docs/ai_smoke.py
   ```

Har check ek ✅ ya ❌ print karta hai; end me summary. Koi real key script me nahi hoti —
aap apni key `API_KEY` env var me de sakte hain (default `public`).

## Kya cover hota hai
- Provider registry, model add, role assign, credential store (key never answered)
- Streaming chat + skills + tool loop (`node.create` → repair → `node.set`) se canvas pe design
- Named AI actions, chat history, diagnostics, usage
- MCP client (keyless public server), media tools (no-key image sourcing: Openverse + Wikimedia),
  vectorize, remove-background
- Accounts + cloud storage (register → sign-in → doc round-trip) — local server ke saath
- Browser build ka `available() == false` guard
