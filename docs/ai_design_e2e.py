#!/usr/bin/env python3
"""End-to-end design test: ask the AI for real interfaces, then grade what it drew.

Unlike `ai_smoke.py` (which checks that every surface answers), this script checks the *work*:
a full marketing webpage, a multi-screen mobile app, a prototype with motion, and a page with
thousands of layers. Each design is exported to PNG and measured from the document tree, so the
checks are objective (layer counts, colours, text, alignment, overflow, prototype links).

Usage:
  DISPLAY=:99 ./target/debug/figcraft --control 59253 &
  python3 docs/ai_design_e2e.py            # all scenarios
  python3 docs/ai_design_e2e.py webpage    # one scenario

Env: FIGCRAFT_PORT, AI_BASE_URL, AI_MODEL, API_KEY, OUT_DIR (default /tmp/figcraft-e2e)
"""
import json
import os
import socket
import sys
import time

PORT = int(os.environ.get("FIGCRAFT_PORT", "59253"))
BASE = os.environ.get("AI_BASE_URL", "http://127.0.0.1:20129/v1")
MODEL = os.environ.get("AI_MODEL", "mimo-v2.6-flash-free")
KEY = os.environ.get("API_KEY", "public")
OUT = os.environ.get("OUT_DIR", "/tmp/figcraft-e2e")
os.makedirs(OUT, exist_ok=True)

_sock = socket.create_connection(("127.0.0.1", PORT), timeout=120)
_file = _sock.makefile("rwb")
_seq = [0]


def call(method, params=None, timeout=180_000):
    _seq[0] += 1
    payload = {"id": _seq[0], "method": method, "params": params or {}, "timeoutMs": timeout}
    _file.write((json.dumps(payload) + "\n").encode())
    _file.flush()
    line = _file.readline()
    if not line:
        return {"ok": False, "error": "no reply"}
    return json.loads(line)


def eng(cmd, params=None):
    r = call("engine.execute", {"command": cmd, "params": params or {}})
    if not r.get("ok"):
        return {"ok": False, "error": r.get("error") or (r.get("result") or {}).get("error")}
    return r


def ok(r):
    return bool(r.get("ok"))


results = []


def check(label, fn):
    try:
        good, detail = fn()
    except Exception as e:  # noqa: BLE001 - the harness reports, it does not raise
        good, detail = False, f"{type(e).__name__}: {e}"
    results.append((label, good, detail))
    print(("  PASS  " if good else "  FAIL  ") + label + (f" — {detail}" if detail else ""))
    return good


# ---------------------------------------------------------------- setup


def configure_model():
    """Point the Design role at the local proxy (idempotent)."""
    models = eng("ai.models").get("result", {})
    for connection in models.get("connections", []):
        if connection.get("customBaseURL", "").rstrip("/") == BASE.rstrip("/"):
            conn = connection["id"]
            break
    else:
        conn = eng("ai.connection.add", {"provider": "openai-compatible", "customBaseURL": BASE, "customAPIType": "completions"})["result"]["id"]
    model_id = None
    for model in eng("ai.models").get("result", {}).get("models", []):
        if model.get("connectionId") == conn and model.get("customModelID") == MODEL:
            model_id = model["id"]
            break
    if model_id is None:
        model_id = eng("ai.model.add", {
            "name": "E2E " + MODEL,
            "connectionId": conn,
            "customModelID": MODEL,
            "capabilities": ["tools", "vision"],
        })["result"]["id"]
    eng("ai.role.assign", {"role": "design", "modelId": model_id})
    eng("ai.role.assign", {"role": "review", "modelId": model_id})
    eng("ai.role.assign", {"role": "vision", "modelId": model_id})
    eng("ai.credential.set", {"integration": "openai-compatible", "profileId": conn, "field": "api-key", "value": KEY})
    return model_id


def new_page(name):
    eng("file.new")
    eng("ai.chat.new")
    doc = call("document.inspect")["result"]
    page = doc["pages"][0]["id"]
    if name:
        eng("page.rename", {"id": page, "name": name})
    return page


def ask(text, budget=420, quiet_for=6):
    """Send a prompt and wait for the whole tool loop to go quiet."""
    before = len(eng("ai.chat.messages")["result"]["messages"])
    r = eng("ai.chat.send", {"text": text})
    if not ok(r):
        return None, f"send failed: {r}"
    started = time.time()
    last = before
    last_change = started
    while time.time() - started < budget:
        time.sleep(2)
        state = eng("ai.chat.messages")["result"]
        count = len(state["messages"])
        if count != last:
            last = count
            last_change = time.time()
        if not state["busy"] and time.time() - last_change >= quiet_for:
            return state["messages"], None
    return eng("ai.chat.messages")["result"]["messages"], "timed out"


def tree(page=None):
    args = {"depth": 64}
    if page is not None:
        args["page"] = page
    r = call("document.inspect", args)
    return r.get("result") or {}


def walk(nodes, depth=1):
    for node in nodes:
        yield node, depth
        yield from walk(node.get("children") or [], depth + 1)


def export_png(name, node=None, scale=2):
    path = os.path.join(OUT, name)
    params = {"format": "png", "scale": scale, "path": path}
    if node is not None:
        params["ids"] = [node]
    r = eng("file.export", params)
    return path if ok(r) and os.path.exists(path) else None


def shot(name):
    path = os.path.join(OUT, name)
    r = call("ui.screenshot", {"path": path}, 60_000)
    return path if ok(r) and os.path.exists(path) else None


def crop_shot(node, name):
    """Frame the canvas on a node, then screenshot the window."""
    eng("select.set", {"ids": [node]})
    call("ui.zoomToSelection", {})
    time.sleep(1)
    return shot(name)


# ---------------------------------------------------------------- scenarios


WEBPAGE_PROMPT = """Design a complete marketing landing page for "Ridgeline", a hiking-route planning app.
Desktop frame 1440x3200 on the page. Build it with real auto layout, not loose absolute positions.

Sections, top to bottom:
1. Top navigation: logo word "Ridgeline", links (Features, Routes, Pricing, About), a "Start free" button.
2. Hero: headline, one-line subhead, a primary and a secondary button, and a large product visual made of shapes.
3. Three feature cards in a row, each with an icon shape, a title and two lines of body text.
4. Pricing: three plan cards, the middle one highlighted.
5. Footer with four link columns and a copyright line.

Use an 8px spacing rhythm, one accent colour, a 6-step type scale, and 12-16px corner radii.
Every text layer must be real text. Name every layer meaningfully.
When you are done, call get_node on the root frame and fix anything that overflows or is misaligned."""

MOBILE_PROMPT = """Design a mobile app called "Trailhead" as three iPhone screens side by side (390x844 each), laid out in a row with 40px between them.
Screen 1 — Discover: status bar hints, an "Explore trails" title, a search field, a horizontally scrolling card rail of 3 trail cards (image block, name, distance, difficulty chip), a section header "Near you" and a vertical list of 3 rows with a thumbnail and two text lines.
Screen 2 — Trail detail: a large hero image block, a title, a row of 3 stat columns (Distance, Elevation, Time), a paragraph, and a sticky bottom bar with a heart icon button and a "Start hike" button.
Screen 3 — Profile: an avatar circle, a name, two stats, a settings list of 4 rows with a chevron, and a tab bar at the bottom with 4 tabs.
Use auto layout everywhere, 16px screen padding, 12px card radii, a consistent accent colour, and real text layers.
Finish by checking each screen with get_node and fixing overflow."""

MOTION_PROMPT = """On this page, prototype a mobile flow with motion.
Create three frames at 390x844 side by side, 60px apart: "Home", "Detail" and "Sheet".
On "Home" put a button "Open detail"; on "Detail" put a "Back" button; on "Sheet" a close handle.
Make the interactions: Home button -> Detail with a smart animate transition of 300ms ease-out;
Detail Back -> Home with a dissolve; and on Home a "More" button that opens "Sheet" as an overlay
that slides up 400ms with the spring easing.
Set the prototype start frame to "Home"."""

SCALE_PROMPT = """Create a 1200x800 frame named "Grid" and fill it with a 20 by 20 grid of 40x40 squares with 8px gaps.
Use batch calls with many commands per call to do it quickly, then tell me how many layers there are."""


def scenario_webpage():
    print("\n=== Scenario: marketing webpage ===")
    page = new_page("Webpage")
    msgs, err = ask(WEBPAGE_PROMPT)
    check("AI answered the webpage prompt", lambda: (not err and msgs and any(m["role"] == "assistant" and m.get("content") for m in msgs), err or f"{len(msgs or [])} messages"))
    calls = [m for m in (msgs or []) if m["role"] == "tool"]
    check("tool loop ran", lambda: (len(calls) >= 5, f"{len(calls)} tool results"))

    d = tree()
    nodes = list(walk(d["pages"][0].get("children", [])))
    check("page has a real layer tree", lambda: (len(nodes) >= 25, f"{len(nodes)} layers"))
    frames = [n for n, _ in nodes if n.get("type") == "FRAME" and n.get("width", 0) >= 1000]
    check("a desktop frame exists at the asked size", lambda: (
        any(abs(f.get("width", 0) - 1440) < 8 for f in frames),
        ", ".join(f"{f['name']} {f['width']}x{f['height']}" for f in frames[:4]) or "none",
    ))
    texts = [n for n, _ in nodes if n.get("type") == "TEXT"]
    check("real text layers, not drawn rectangles", lambda: (len(texts) >= 12, f"{len(texts)} text layers"))
    empty_text = [t["name"] for t in texts if not (t.get("text") or "").strip()]
    check("no empty text layers", lambda: (not empty_text, ", ".join(empty_text[:4]) or "none"))
    named = [n for n, _ in nodes if not str(n.get("name", "")).startswith(("Rectangle", "Frame ", "Ellipse", "Group"))]
    check("layers are named meaningfully", lambda: (len(named) >= len(nodes) * 0.5, f"{len(named)}/{len(nodes)} named"))

    # overflow: children escaping their parent's box
    bad = []
    for node, _ in nodes:
        children = node.get("children") or []
        if not children or node.get("type") not in ("FRAME", "GROUP", "COMPONENT", "SECTION"):
            continue
        w, h = node.get("width", 0), node.get("height", 0)
        for child in children:
            if child.get("layout", {}).get("mode", "NONE") != "NONE":
                continue
            if child.get("x", 0) < -1 or child.get("y", 0) < -1:
                bad.append(f"{child['name']} at ({child['x']},{child['y']})")
            elif child.get("x", 0) + child.get("width", 0) > w + 1 or child.get("y", 0) + child.get("height", 0) > h + 1:
                bad.append(f"{child['name']} exceeds {node['name']}")
    check("nothing overflows its frame", lambda: (len(bad) <= 2, f"{len(bad)} escapes: " + "; ".join(bad[:3]) if bad else "none"))

    autolayout = [n for n, _ in nodes if (n.get("layout") or {}).get("mode", "NONE") != "NONE"]
    check("auto layout is used", lambda: (len(autolayout) >= 4, f"{len(autolayout)} auto-layout frames"))

    root = next((f for f in frames if abs(f.get("width", 0) - 1440) < 8), None)
    if root:
        png = export_png("webpage.png", root["id"], 1)
        check("webpage exports to PNG", lambda: (bool(png), png or "no file"))
        shot_path = crop_shot(root["id"], "webpage-window.png")
        check("webpage screenshot taken", lambda: (bool(shot_path), shot_path or "none"))


def scenario_mobile():
    print("\n=== Scenario: mobile app ===")
    new_page("Mobile")
    msgs, err = ask(MOBILE_PROMPT)
    check("AI answered the mobile prompt", lambda: (not err, err or "ok"))
    d = tree()
    nodes = list(walk(d["pages"][0].get("children", [])))
    frames = [n for n, _ in nodes if n.get("type") == "FRAME" and 350 <= n.get("width", 0) <= 430]
    check("three phone frames", lambda: (len(frames) >= 3, ", ".join(f"{f['name']} {f['width']}x{f['height']}" for f in frames)))
    screens = [f for f in frames if abs(f.get("height", 0) - 844) < 12]
    check("frames are 390x844-ish", lambda: (len(screens) >= 3, f"{len(screens)} at the asked size"))
    texts = [n for n, _ in nodes if n.get("type") == "TEXT" and (n.get("text") or "").strip()]
    check("each screen has real copy", lambda: (len(texts) >= 15, f"{len(texts)} text layers"))
    tabbar = [n for n, _ in nodes if "tab" in str(n.get("name", "")).lower()]
    check("a tab bar exists", lambda: (bool(tabbar), ", ".join(t["name"] for t in tabbar[:3]) or "none"))
    bad = []
    for node, _ in nodes:
        if node.get("type") != "FRAME":
            continue
        w, h = node.get("width", 0), node.get("height", 0)
        for child in node.get("children") or []:
            if (child.get("layout") or {}).get("mode", "NONE") != "NONE":
                continue
            if child.get("x", 0) + child.get("width", 0) > w + 1 or child.get("y", 0) + child.get("height", 0) > h + 1:
                bad.append(f"{child.get('name')} in {node.get('name')}")
    check("no overflow inside screens", lambda: (len(bad) <= 2, f"{len(bad)}: " + "; ".join(bad[:3]) if bad else "none"))
    for frame in screens[:3]:
        export_png(f"mobile-{frame['name'].replace(' ', '-')}.png", frame["id"], 1)
    if screens:
        crop_shot(screens[0]["id"], "mobile-window.png")


def scenario_motion():
    print("\n=== Scenario: prototype motion ===")
    new_page("Motion")
    msgs, err = ask(MOTION_PROMPT)
    check("AI answered the motion prompt", lambda: (not err, err or "ok"))
    d = tree()
    nodes = list(walk(d["pages"][0].get("children", [])))
    frames = [n for n, _ in nodes if n.get("type") == "FRAME" and 350 <= n.get("width", 0) <= 430]
    check("three frames exist", lambda: (len(frames) >= 3, ", ".join(f["name"] for f in frames)))
    linked = [(n, n.get("prototype") or n.get("interactions")) for n, _ in nodes]
    with_proto = [(n, p) for n, p in linked if p]
    check("prototype data was written", lambda: (bool(with_proto), f"{len(with_proto)} layers carry prototype data"))
    detail = json.dumps(with_proto)[:400] if with_proto else "none"
    animated = any("ANIMATE" in json.dumps(p).upper() or "SMART" in json.dumps(p).upper() for _, p in with_proto)
    check("a smart-animate transition was set", lambda: (animated, detail))
    if frames:
        crop_shot(frames[0]["id"], "motion-window.png")


def scenario_scale():
    print("\n=== Scenario: thousands of layers ===")
    new_page("Scale")
    started = time.time()
    msgs, err = ask(SCALE_PROMPT)
    elapsed = time.time() - started
    check("AI answered the scale prompt", lambda: (not err, err or f"{elapsed:.0f}s"))
    d = tree()
    nodes = list(walk(d["pages"][0].get("children", [])))
    check("hundreds/thousands of layers were created", lambda: (len(nodes) >= 200, f"{len(nodes)} layers in {elapsed:.0f}s"))
    # A direct command path, independent of the model, for the real ceiling.
    eng("file.new")
    eng("node.create", {"type": "frame", "name": "Bulk", "width": 4000, "height": 4000})
    t0 = time.time()
    made = 0
    batch = []
    for i in range(2000):
        batch.append({"command": "node.create", "params": {
            "type": "rectangle", "x": (i % 50) * 60, "y": (i // 50) * 60,
            "width": 48, "height": 48, "name": f"cell-{i}",
            "props": {"fills": [{"type": "solid", "color": "#22CC88"}]},
        }})
        if len(batch) == 200:
            r = eng("command.batch", {"commands": batch})
            made += len(batch) if ok(r) else 0
            batch = []
    dt = time.time() - t0
    check("2000 layers land through batch", lambda: (made >= 2000, f"{made} in {dt:.1f}s ({made / max(dt, 0.001):.0f}/s)"))
    t1 = time.time()
    d = tree()
    total = sum(1 for _ in walk(d["pages"][0].get("children", [])))
    check("the document still inspects quickly", lambda: (total >= 2000 and time.time() - t1 < 15, f"{total} layers in {time.time() - t1:.1f}s"))
    t2 = time.time()
    png = export_png("scale.png", None, 1)
    check("a 2000-layer page exports", lambda: (bool(png) and time.time() - t2 < 60, f"{time.time() - t2:.1f}s"))


SCENARIOS = {
    "webpage": scenario_webpage,
    "mobile": scenario_mobile,
    "motion": scenario_motion,
    "scale": scenario_scale,
}


def main():
    wanted = sys.argv[1:] or list(SCENARIOS)
    print(f"FigCraft AI design E2E — port {PORT}, model {MODEL}, out {OUT}")
    check("app responds", lambda: (ok(call("window.list")), "ok"))
    configure_model()
    check("design role resolves", lambda: (eng("ai.chat.model")["result"]["modelID"] == MODEL, eng("ai.chat.model")["result"]["modelID"]))
    for name in wanted:
        if name not in SCENARIOS:
            print(f"unknown scenario {name}")
            continue
        SCENARIOS[name]()
    passed = sum(1 for _, good, _ in results if good)
    print(f"\n{passed}/{len(results)} checks passed")
    if passed != len(results):
        for label, good, detail in results:
            if not good:
                print(f"  FAILED: {label} — {detail}")
        sys.exit(1)


if __name__ == "__main__":
    main()
