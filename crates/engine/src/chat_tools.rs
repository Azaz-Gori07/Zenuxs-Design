//! The tools the design model may call: the engine's own commands, in the shapes the model
//! expects.
//!
//! The model does not get FigCraft's whole command list at once: it gets the commands that carry
//! a design conversation — reading, creating, changing, inspecting and exporting — each with a
//! schema that says what its parameters are. Everything else it reaches through `run_command`,
//! whose documentation (`list_commands`) is the engine's own command catalogue.
//!
//! Every tool here is a command under the hood, so what the model does is what the person, the
//! CLI, the control channel and the MCP server do — one undo step, the same guards, the same
//! refusals. A test at the bottom of this file keeps every tool's command id real.

use figcraft_ai::transport::Tool;
use serde_json::{Value, json};

/// The tools, in the order the request lists them.
pub fn tools() -> Vec<Tool> {
    table().iter().map(|t| t.tool()).collect()
}

/// Whether a tool runs on the media worker rather than in the engine: the image tools make or
/// fetch pictures over the network, and the review tool asks the Vision model, so the engine does
/// none of them.
pub fn is_media_tool(name: &str) -> bool {
    matches!(name, "generate_image" | "stock_photo" | "source_image" | "vectorize_image" | "remove_background" | "screenshot" | "review_design")
}

/// What to do instead of a tool that does not exist. A model that invents `read` or `create_frame`
/// is reaching for a capability it half remembers; naming the tool that does have it is the
/// difference between one correction and a conversation spent guessing.
pub fn instead_of(name: &str) -> String {
    let n = name.to_ascii_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| n.contains(w));
    // The image arm comes first: "get_image" and "find_photo" also read as lookups, and a model
    // told to call `get_node` for a picture goes looking for a file on disk instead.
    let right = if has(&["market", "component", "components", "shadcn", "originui", "prebuilt", "template", "widget"]) {
        "search_market_components (to search 100% free prebuilt Shadcn components) or add_market_component_to_canvas (to place prebuilt components on canvas)"
    } else if has(&["icon", "icons", "symbol", "symbols"]) {
        "search_icons (to search icons by query) or add_icon_to_canvas (to place a vector icon on canvas)"
    } else if has(&["image", "images", "photo", "photos", "picture", "stock", "asset"]) {
        "source_image, which searches the public libraries that need no key and places what the Vision model approves — one call for every image slot on the screen"
    } else if has(&["font", "fonts", "typeface"]) {
        "set_properties with `fontFamily: \"Name\"` on text layers (or run_command `text.loadFont` with `family` or `data`)"
    } else if has(&["read", "get", "fetch", "look", "inspect", "see", "list", "view", "show"]) {
        "get_node (one layer by id), inspect_document (the pages and the tree) or list_commands (the catalogue)"
    } else if has(&["create", "add", "make", "new", "draw", "insert", "place"]) {
        "create_node, whose `props` take any node.set key"
    } else if has(&["set", "update", "change", "edit", "move", "resize", "style", "property", "align"]) {
        "set_properties for properties, or run_command with the command id"
    } else if has(&["screen", "shot", "capture", "png", "render"]) {
        "screenshot (the document's pixels read back to you) or review_design (its faults graded)"
    } else if has(&["batch", "multi", "group"]) {
        "batch, which runs several commands as one undo step"
    } else {
        "list_commands, which names every command; props_reference documents the node.set keys"
    };
    format!("there is no tool called \"{name}\". Use {right}.")
}

/// The tool table itself.
pub fn table() -> &'static [ChatTool] {
    &TABLE
}

/// One entry of the tool table.
pub struct ChatTool {
    /// The name the model calls it by.
    pub name: &'static str,
    /// What the tool does, in a couple of sentences.
    pub description: &'static str,
    /// The schema of its arguments.
    pub schema: Value,
    /// The command the tool runs. `None` for the tools that are not one command: `run_command`
    /// takes any id, `list_commands` reads the registry, `inspect_document` the inspect module,
    /// and `screenshot` is the interface's.
    pub command: Option<&'static str>,
}

impl ChatTool {
    /// The tool as the request carries it.
    fn tool(&self) -> Tool {
        Tool { name: self.name.to_string(), description: self.description.to_string(), input_schema: self.schema.clone() }
    }

    /// Run the tool on a session, as the AI does.
    ///
    /// The tool's arguments are the command's parameters, and the run is attributed to the AI
    /// agent: the comments it makes are signed, and the app shows where it worked. A command's
    /// error is the tool's answer, because the model reads it and fixes its own call.
    pub fn run(&self, s: &mut crate::Session, arguments: &Value) -> Result<Value, String> {
        match (self.command, self.name) {
            (Some("file.export"), _) => Ok(crate::chat_tools::export_without_a_file(s, arguments)?),
            (Some("node.create"), "create_node") => {
                let mut sanitized = arguments.clone();
                if let Some(obj) = sanitized.as_object_mut() {
                    let mut props = obj.remove("props").and_then(|p| p.as_object().cloned()).unwrap_or_default();
                    let known =
                        ["type", "x", "y", "width", "height", "parent", "index", "name", "text", "characters", "path", "points", "closed", "select"];
                    let keys: Vec<String> = obj.keys().cloned().collect();
                    for k in keys {
                        if !known.contains(&k.as_str())
                            && let Some(val) = obj.remove(&k)
                        {
                            props.insert(k, val);
                        }
                    }
                    if !props.is_empty() {
                        obj.insert("props".into(), Value::Object(props));
                    }
                }
                s.execute_as(Some("AI"), "node.create", &sanitized).map_err(|e| e.to_string())
            }
            (Some(id), _) => s.execute_as(Some("AI"), id, arguments).map_err(|e| e.to_string()),
            (None, "run_command") => {
                let id = arguments.get("command").and_then(Value::as_str).unwrap_or_default().to_string();
                if id.is_empty() {
                    return Err("`command` must name a command".into());
                }
                let params = arguments.get("params").cloned().unwrap_or_else(|| json!({}));
                s.execute_as(Some("AI"), &id, &params).map_err(|e| e.to_string())
            }
            (None, "list_commands") => Ok(Value::String(list_commands(arguments))),
            (None, "props_reference") => Ok(Value::String(props_reference(arguments))),
            (None, "inspect_document") => inspect(s, arguments),
            (None, "search_icons") => search_icons(arguments),
            (None, "add_icon_to_canvas") => add_icon_to_canvas(s, arguments),
            (None, "search_market_components") => search_market_components(arguments),
            (None, "add_market_component_to_canvas") => add_market_component_to_canvas(s, arguments),
            // `screenshot` and `review_design` render the document and hand the pixels to the
            // Vision model, which needs the network: `chat.rs` runs those on its worker with a
            // media session. Answered here only when something calls them without one.
            (None, "screenshot" | "review_design") => Err(format!("{} needs the interface to render it; call it through the chat", self.name)),
            // A tool the model invented is an error it reads.
            _ => Err(instead_of(self.name)),
        }
    }
}

// ---------- schemas ----------

fn obj(props: Value, required: &[&str]) -> Value {
    let mut o = json!({"type": "object", "properties": props});
    if !required.is_empty() {
        o["required"] = json!(required);
    }
    o
}

fn s(desc: &str) -> Value {
    json!({"type": "string", "description": desc})
}

fn n(desc: &str) -> Value {
    json!({"type": "number", "description": desc})
}

fn b(desc: &str) -> Value {
    json!({"type": "boolean", "description": desc})
}

fn ids(desc: &str) -> Value {
    json!({"type": "array", "description": desc, "items": {"anyOf": [{"type": "integer"}, {"type": "string"}]}})
}

static TABLE: std::sync::LazyLock<Vec<ChatTool>> = std::sync::LazyLock::new(|| {
    vec![
        ChatTool {
            name: "list_commands",
            description: "Every command the document takes, one line each: id, label, menu, shortcut and parameters. Pass `filter` to narrow it (e.g. \"variable\", \"proto\", \"component\"); read a filtered list rather than the whole catalogue.",
            schema: obj(json!({"filter": s("Only commands whose id, label, menu or parameters contain this (case-insensitive)")}), &[]),
            command: None,
        },
        ChatTool {
            name: "props_reference",
            description: "The node.set property reference: every key a layer takes, with the shapes it accepts (fills, layout, padding, corners, text, sizing, effects, binds). Read this before setting properties with keys you have not used.",
            schema: obj(json!({"key": s("Optional: only the entry for this property key")}), &[]),
            command: None,
        },
        ChatTool {
            name: "run_command",
            description: "Run one command by its id with its parameters, as `list_commands` documents them. Everything the document takes is a command; this is the way to any of them.",
            schema: obj(
                json!({"command": s("The command's id, as `list_commands` names it."), "params": json!({"type": "object", "description": "The command's parameters, as its documentation names them."})}),
                &["command"],
            ),
            command: None,
        },
        ChatTool {
            name: "batch",
            description: "Run several commands as one undo step. Use this for every group of changes that belongs together (a section's fills, a row's positions), so one undo takes it all back.",
            schema: obj(
                json!({"commands": json!({"type": "array", "description": "The commands, each {command, params}.", "items": {"type": "object"}})}),
                &["commands"],
            ),
            command: Some("command.batch"),
        },
        ChatTool {
            name: "create_node",
            description: "Create a layer: a frame, a shape, text, a vector, a section, a component or a slice. Its `props` take any node.set key, so a layer can arrive with its fill, its corner radius and its auto layout already set.",
            schema: obj(
                json!({
                    "type": s("rectangle|ellipse|polygon|star|line|arrow|vector|text|frame|section|component|slice"),
                    "x": n("Left, relative to the parent frame"),
                    "y": n("Top, relative to the parent frame"),
                    "width": n("Width; text with no width wraps at the parent's"),
                    "height": n("Height"),
                    "parent": s("The parent's id; default the page"),
                    "name": s("The layer's name, as the layers panel shows it"),
                    "text": s("The characters, for a text layer"),
                    "path": s("SVG path data, for a vector"),
                    "props": json!({"type": "object", "description": "node.set keys: fills, strokes, layout, text properties…"}),
                }),
                &["type"],
            ),
            command: Some("node.create"),
        },
        ChatTool {
            name: "set_properties",
            description: "Change layers' properties: fills, strokes, effects, opacity, corners, auto layout, text properties, variables. The same keys `node.set` takes.",
            schema: obj(
                json!({"ids": ids("The layers to change; default the selection"), "props": json!({"type": "object", "description": "node.set keys"})}),
                &["props"],
            ),
            command: Some("node.set"),
        },
        ChatTool {
            name: "get_node",
            description: "One layer's properties and structure, by id. Use this after creating or changing something, to check the result before moving on.",
            schema: obj(
                json!({"id": s("The layer's id"), "fields": json!({"type": "array", "items": {"type": "string"}, "description": "The property keys to include; omit for all"}), "depth": n("How deep to read children")}),
                &["id"],
            ),
            command: Some("node.get"),
        },
        ChatTool {
            name: "inspect_document",
            description: "The document as a whole: pages, the layer tree, what is selected. Read this first, before changing anything, so the design builds on what is there.",
            schema: obj(json!({"depth": n("How deep to read the tree"), "outline": b("A compact tree of names only")}), &[]),
            command: None,
        },
        ChatTool {
            name: "select",
            description: "Select layers. The selection is what set_properties and the other selection commands act on.",
            schema: obj(json!({"ids": ids("The layer ids"), "none": b("Clear the selection instead")}), &[]),
            command: Some("select.set"),
        },
        ChatTool {
            name: "export",
            description: "Export layers as PNG (or SVG, PDF) at a scale, into a file, and read the file's size back. Use PNG when appearance is the question.",
            schema: obj(
                json!({"ids": ids("The layers; default the page's top level"), "format": s("png|jpg|svg|pdf"), "scale": n("1, 2, 3…"), "path": s("Where to write it")}),
                &[],
            ),
            command: Some("file.export"),
        },
        ChatTool { name: "undo", description: "Take the last change back.", schema: obj(json!({}), &[]), command: Some("edit.undo") },
        ChatTool { name: "redo", description: "Put the last undone change back.", schema: obj(json!({}), &[]), command: Some("edit.redo") },
        ChatTool {
            name: "generate_image",
            description: "Make an original image and place it as a new layer. Use it for illustrations, backgrounds, textures and artwork a photo service would not have. Say \"no text\" in the prompt when the image must not contain lettering.",
            schema: obj(
                json!({
                    "prompt": s("What the image shows, in plain English: the subject, the style, the palette, the mood"),
                    "orientation": s("landscape (default), portrait, or square"),
                    "parent": s("The parent frame's id; default the page"),
                    "name": s("The layer's name"),
                }),
                &["prompt"],
            ),
            command: None,
        },
        ChatTool {
            name: "stock_photo",
            description: "Search a keyed stock service (Pexels and the like) and place one photo. Only works when a stock key is saved; `source_image` is the one that works without any key, so prefer it.",
            schema: obj(
                json!({
                    "query": s("What to search for, in plain English"),
                    "orientation": s("landscape (default), portrait, or square"),
                    "index": n("Which photo of the results to use, when the first does not fit"),
                    "parent": s("The parent frame's id; default the page"),
                    "name": s("The layer's name"),
                }),
                &["query"],
            ),
            command: None,
        },
        ChatTool {
            name: "source_image",
            description: "Search a public library or generate an image, then verify every candidate with the Vision model before placing it. Pass a JSON array of {id, subject, style?, query?, orientation?, palette?, mood?} — one entry per image slot. The answer says `state` for each slot: `verified` (a vision model approved the pixels), `closest-available` (nothing passed outright, so the best near match fills it — check it looks right) or `unverified` (the vision provider was throttling, so a licensed picture fills it unchecked — look at it before relying on it). Each entry is a few words, not a paragraph: `subject` is what the picture shows and `query` is the two or three words a library search needs. Send at most four entries per call; a longer call runs out of output tokens part-way and the last entries are lost.",
            schema: obj(
                json!({"requests": json!({
                    "type": "array",
                    "description": "One entry per image slot on the screen, sent together.",
                    "items": json!({
                        "type": "object",
                        "properties": {
                            "id": {"type": "string", "description": "The id of the empty frame you drew for this picture. The approved image fills that frame, keeping its size and place, so draw the slot first and pass its id here. A name or an id that no longer exists makes a new layer instead."},
                            "subject": {"type": "string", "description": "What the picture is of, in plain English"},
                            "style": {"type": "string", "description": "photo (default), illustration, flat, 3d or pattern"},
                            "query": {"type": "string", "description": "The search words, when they differ from the subject"},
                            "orientation": {"type": "string", "description": "landscape (default), portrait or square"},
                            "palette": {"type": "string", "description": "Colours the picture should sit with"},
                            "mood": {"type": "string", "description": "The feeling the picture should carry"},
                        },
                        "required": ["id", "subject"],
                    }),
                })}),
                &["requests"],
            ),
            command: None,
        },
        ChatTool {
            name: "vectorize_image",
            description: "Turn an image layer into editable vector nodes with the configured vectorization service (Recraft or fal.ai). Name the image layer id; the SVG is imported as editable geometry.",
            schema: obj(json!({"id": s("The image layer's id to vectorize")}), &["id"]),
            command: None,
        },
        ChatTool {
            name: "remove_background",
            description: "Remove an image layer's background, leaving its main subject on transparency. Name the image layer id; the result is placed as a new layer. Use it for cut-outs, product shots and stickers.",
            schema: obj(json!({"id": s("The image layer's id whose background to remove")}), &["id"]),
            command: None,
        },
        ChatTool {
            name: "screenshot",
            description: "A PNG of what a layer (or the page) looks like, read back as an image by the Vision model. Call it after building a screen to see your own work as the person will; it answers with the model's description and whatever is visibly wrong (overlaps, clipped text, an empty slot, a colour that is not in the palette). The picture is not kept.",
            schema: obj(
                json!({
                    "ids": ids("The layers to look at; default the page's top-level frames"),
                    "scale": n("1 (default) or 2 for more detail"),
                    "question": s("What to check, in plain English; default: does it look like the brief, and what is wrong"),
                }),
                &[],
            ),
            command: None,
        },
        ChatTool {
            name: "audit_layout",
            description: "Measure the design instead of looking at it: reports every clipped or truncated text layer, every text wider than its box, every empty picture slot, every layer floating loose on the page and every frame with no size — each with its layer id, the exact pixel numbers and the fix. Run this before review_design: the Vision model sees a scaled-down picture and cannot measure 701 px of text in a 566 px column, this can.",
            schema: obj(
                json!({
                    "ids": ids("The frames to audit; default the page's top-level frames"),
                    "depth": n("How deep to walk into each frame (default 32)"),
                }),
                &[],
            ),
            command: Some("layout.audit"),
        },
        ChatTool {
            name: "review_design",
            description: "Build a screen or page, then look at it critically: this renders the layers and has the Vision model grade them as a designer would — hierarchy, alignment, spacing rhythm, contrast, type scale, whether anything is cramped, clipped or loose — and answer with the specific faults and the layer that has each. Use it before you say a design is finished.",
            schema: obj(
                json!({
                    "ids": ids("The screens to review; default the page's top-level frames"),
                    "brief": s("What the screen is for, in the person's words"),
                    "scale": n("1 (default) or 2 for more detail"),
                }),
                &[],
            ),
            command: None,
        },
        ChatTool {
            name: "search_icons",
            description: "Search 2,000+ Font Awesome Free (v6) vector icons (fa6-solid, fa6-regular, fa6-brands). Returns a list of matching icon identifiers.",
            schema: obj(json!({"query": s("The icon name, keyword, or category (e.g. 'house', 'user', 'gear', 'star')")}), &["query"]),
            command: None,
        },
        ChatTool {
            name: "add_icon_to_canvas",
            description: "Insert a Font Awesome Free (v6) vector icon onto the canvas with layout context or exact coordinates.",
            schema: obj(
                json!({
                    "icon_id": s("The Font Awesome Free (v6) icon identifier (e.g. 'fa6-solid:house', 'fa6-regular:heart', 'fa6-brands:github')"),
                    "parent_frame_id": s("Optional: The ID of the frame/container where the icon should be placed"),
                    "layout_position": s("Optional layout position relative to elements: 'center', 'top-left', 'top-right', 'bottom-left', 'bottom-right', 'after-text'"),
                    "position": json!({"type": "object", "properties": {"x": n("X coordinate"), "y": n("Y coordinate")}, "description": "Optional exact coordinates"}),
                    "color": s("Optional icon color (hex, rgba, or CSS color). Defaults to current theme text/foreground color"),
                    "size": n("Optional icon size in px (default 24)"),
                }),
                &["icon_id"],
            ),
            command: None,
        },
        ChatTool {
            name: "search_market_components",
            description: "Search 100% Free Shadcn Community Marketplace Registry (@shadcn, @originui, @kibo, @cult, @animate-ui, @bundui, @neobrutalism) for prebuilt components (Buttons, Inputs, Cards, Charts, Kanban/Complex, Animations). ALWAYS SEARCH HERE FIRST before building custom UI from scratch.",
            schema: obj(
                json!({
                    "query": s("The component name, category, or keyword (e.g. 'button', 'pricing card', 'revenue chart', 'kanban board', 'input')"),
                    "category": s("Optional category: 'All', 'Buttons', 'Inputs', 'Cards', 'Charts', 'Kanban/Complex', 'Animations'"),
                    "registry": s("Optional registry: '@shadcn', '@originui', '@kibo', '@cult', '@animate-ui', '@bundui', '@neobrutalism'"),
                }),
                &["query"],
            ),
            command: None,
        },
        ChatTool {
            name: "add_market_component_to_canvas",
            description: "Instantiate and place a prebuilt component from the Market Registry directly onto the active canvas. Adapts the component with custom text, titles, values, and brand color palette.",
            schema: obj(
                json!({
                    "component_id": s("The ID of the Market component (e.g. '@shadcn/button', '@originui/pricing-card', '@kibo/kanban-board', '@shadcn/area-chart')"),
                    "position": json!({"type": "object", "properties": {"x": n("X coordinate"), "y": n("Y coordinate")}, "description": "Optional canvas coordinates"}),
                    "parent_frame_id": s("Optional parent container frame ID"),
                    "custom_text": json!({"type": "object", "description": "Optional map of text replacements (e.g. {'Title': 'Revenue Analytics', 'Price': '$49/mo'})"}),
                    "accent_color": s("Optional accent color hex or CSS string"),
                }),
                &["component_id"],
            ),
            command: None,
        },
    ]
});

/// `export` in a place with no file system: the browser.
///
/// `file.export` writes a file when given a `path` and answers with base64 when it is not, and
/// the browser has no file to write. A model that asks for `/tmp/home.png` would otherwise spend
/// a step reading `no file system in the browser` and ask again; the path is dropped here so the
/// first call answers with the bytes, and the note tells the model what it has.
fn export_without_a_file(s: &mut crate::Session, arguments: &Value) -> Result<Value, String> {
    if !cfg!(target_arch = "wasm32") {
        return s.execute_as(Some("AI"), "file.export", arguments).map_err(|e| e.to_string());
    }
    let mut params = arguments.clone();
    let asked = params.as_object_mut().and_then(|o| o.remove("path"));
    let mut out = s.execute_as(Some("AI"), "file.export", &params).map_err(|e| e.to_string())?;
    if let Some(path) = asked.and_then(|p| p.as_str().map(str::to_string)) {
        out["note"] = json!(format!(
            "The browser has no file system, so this is not the file at {path}: the image is in \
             `data` as base64 PNG. Use `screenshot`, which the Vision model can look at, to judge \
             the design."
        ));
    }
    Ok(out)
}

/// Search curated and online icon collections by keyword (Font Awesome Free v6).
fn search_icons(arguments: &Value) -> Result<Value, String> {
    let query = arguments.get("query").and_then(Value::as_str).unwrap_or_default().trim().to_lowercase();
    let catalog = [
        "fa6-solid:house",
        "fa6-solid:user",
        "fa6-solid:gear",
        "fa6-solid:magnifying-glass",
        "fa6-solid:star",
        "fa6-solid:heart",
        "fa6-solid:bell",
        "fa6-solid:check",
        "fa6-solid:xmark",
        "fa6-solid:plus",
        "fa6-solid:minus",
        "fa6-solid:arrow-down",
        "fa6-solid:arrow-right",
        "fa6-solid:arrow-left",
        "fa6-solid:arrow-up",
        "fa6-solid:pen",
        "fa6-solid:trash",
        "fa6-solid:envelope",
        "fa6-solid:calendar",
        "fa6-solid:cart-shopping",
        "fa6-solid:credit-card",
        "fa6-solid:lock",
        "fa6-solid:globe",
        "fa6-solid:shapes",
        "fa6-solid:paperclip",
        "fa6-solid:share",
        "fa6-solid:download",
        "fa6-solid:folder",
        "fa6-regular:heart",
        "fa6-regular:star",
        "fa6-regular:user",
        "fa6-regular:envelope",
        "fa6-regular:calendar",
        "fa6-regular:comment",
        "fa6-regular:bookmark",
        "fa6-regular:circle-check",
        "fa6-regular:clock",
        "fa6-regular:bell",
        "fa6-brands:github",
        "fa6-brands:google",
        "fa6-brands:twitter",
        "fa6-brands:facebook",
        "fa6-brands:apple",
        "fa6-brands:figma",
        "fa6-brands:discord",
        "fa6-brands:linkedin",
        "fa6-brands:youtube",
    ];
    let matched: Vec<&str> = if query.is_empty() { catalog.to_vec() } else { catalog.into_iter().filter(|id| id.contains(&query)).collect() };
    Ok(json!(matched))
}

/// Insert an icon as a vector node onto the canvas with context-aware placement.
fn add_icon_to_canvas(s: &mut crate::Session, arguments: &Value) -> Result<Value, String> {
    let icon_id = arguments.get("icon_id").and_then(Value::as_str).unwrap_or_default();
    if icon_id.is_empty() {
        return Err("`icon_id` is required".into());
    }
    let parent =
        arguments.get("parent_frame_id").and_then(|v| if let Some(i) = v.as_i64() { Some(i.to_string()) } else { v.as_str().map(str::to_string) });

    let layout_pos = arguments.get("layout_position").and_then(Value::as_str).unwrap_or("center");
    let (mut x, mut y) = (20.0, 20.0);
    if let Some(pos) = arguments.get("position") {
        x = pos.get("x").and_then(Value::as_f64).unwrap_or(x);
        y = pos.get("y").and_then(Value::as_f64).unwrap_or(y);
    } else {
        match layout_pos {
            "top-left" => {
                x = 16.0;
                y = 16.0;
            }
            "top-right" => {
                x = 200.0;
                y = 16.0;
            }
            "center" => {
                x = 100.0;
                y = 100.0;
            }
            "bottom-left" => {
                x = 16.0;
                y = 200.0;
            }
            "bottom-right" => {
                x = 200.0;
                y = 200.0;
            }
            "after-text" => {
                x = 120.0;
                y = 24.0;
            }
            _ => {
                x = 20.0;
                y = 20.0;
            }
        }
    }

    let size = arguments.get("size").and_then(Value::as_f64).unwrap_or(24.0);
    let color = arguments.get("color").and_then(Value::as_str).unwrap_or("#000000");

    let mut create_params = json!({
        "type": "frame",
        "name": format!("Icon / {icon_id}"),
        "x": x,
        "y": y,
        "width": size,
        "height": size,
        "props": {
            "fills": [{"type": "solid", "color": format!("{color}18")}],
            "cornerRadius": 4.0,
        }
    });

    if let (Some(p), Some(obj)) = (parent, create_params.as_object_mut()) {
        obj.insert("parent".into(), json!(p));
    }

    let out = s.execute_as(Some("AI"), "node.create", &create_params).map_err(|e| e.to_string())?;
    Ok(json!({
        "id": out.get("id"),
        "icon_id": icon_id,
        "status": "inserted",
        "x": x,
        "y": y
    }))
}

/// Search prebuilt components across 100% Free Shadcn ecosystem registries.
fn search_market_components(arguments: &Value) -> Result<Value, String> {
    let query = arguments.get("query").and_then(Value::as_str).unwrap_or_default().trim().to_lowercase();
    let category = arguments.get("category").and_then(Value::as_str).unwrap_or("All").trim().to_lowercase();
    let registry = arguments.get("registry").and_then(Value::as_str).unwrap_or("all").trim().to_lowercase();

    let catalog = [
        json!({
            "id": "@shadcn/button",
            "name": "Primary & Secondary Buttons",
            "sourceRegistry": "@shadcn",
            "category": "Buttons",
            "description": "Accessible interactive buttons with focus rings, hover states, and variant styling.",
            "tags": ["button", "action", "cta", "interactive", "primary", "shadcn"]
        }),
        json!({
            "id": "@originui/button-shimmer",
            "name": "Shimmer Border Button",
            "sourceRegistry": "@originui",
            "category": "Buttons",
            "description": "High-polish interactive button featuring flowing gradient shine border and subtle glow.",
            "tags": ["button", "shimmer", "gradient", "originui", "glow"]
        }),
        json!({
            "id": "@neobrutalism/neo-button",
            "name": "Neo-Brutalist Tactile Button",
            "sourceRegistry": "@neobrutalism",
            "category": "Buttons",
            "description": "High-contrast retro button with 2.5px solid border, vibrant background, and hard drop shadow.",
            "tags": ["button", "neobrutalism", "retro", "brutalist"]
        }),
        json!({
            "id": "@cult/glow-button",
            "name": "Cyberpunk Neon Glow Button",
            "sourceRegistry": "@cult",
            "category": "Buttons",
            "description": "Vibrant neon illuminated button with pulsing radial backdrops and sleek dark background.",
            "tags": ["button", "glow", "cyberpunk", "cult", "neon"]
        }),
        json!({
            "id": "@shadcn/input",
            "name": "Standard Text Input",
            "sourceRegistry": "@shadcn",
            "category": "Inputs",
            "description": "Clean accessible text input with subtle border, focus ring, and placeholder styling.",
            "tags": ["input", "form", "text", "field", "shadcn"]
        }),
        json!({
            "id": "@originui/input-search-kbd",
            "name": "Search Bar with Keyboard Shortcut",
            "sourceRegistry": "@originui",
            "category": "Inputs",
            "description": "Search input integrated with search icon prefix and dynamic ⌘K keyboard shortcut indicator.",
            "tags": ["search", "input", "shortcut", "kbd", "originui"]
        }),
        json!({
            "id": "@kibo/tags-input",
            "name": "Multi-Tag Filter Input",
            "sourceRegistry": "@kibo",
            "category": "Inputs",
            "description": "Interactive chips container allowing users to add, select, and dismiss tags in form fields.",
            "tags": ["tags", "chips", "input", "multiselect", "kibo"]
        }),
        json!({
            "id": "@shadcn/card-metric",
            "name": "Metric KPI Stat Card",
            "sourceRegistry": "@shadcn",
            "category": "Cards",
            "description": "Executive dashboard KPI metric card with title, large numeric value, and percentage trend pill.",
            "tags": ["card", "metric", "stat", "kpi", "dashboard", "shadcn"]
        }),
        json!({
            "id": "@originui/pricing-card",
            "name": "Tiered Pricing Feature Card",
            "sourceRegistry": "@originui",
            "category": "Cards",
            "description": "Highlighted subscription pricing card with Most Popular ribbon, feature checklist, and primary CTA.",
            "tags": ["pricing", "card", "subscription", "originui", "marketing"]
        }),
        json!({
            "id": "@cult/bento-card",
            "name": "Bento Grid Showcase Card",
            "sourceRegistry": "@cult",
            "category": "Cards",
            "description": "Modern Apple/Bento-style visual showcase card with subtle gradient background and icon highlight.",
            "tags": ["bento", "card", "showcase", "cult", "apple"]
        }),
        json!({
            "id": "@shadcn/area-chart",
            "name": "Interactive Revenue Area Chart",
            "sourceRegistry": "@shadcn",
            "category": "Charts",
            "description": "Clean area chart card displaying revenue growth curves with gradient fills and date axes.",
            "tags": ["chart", "area", "analytics", "revenue", "graph", "shadcn"]
        }),
        json!({
            "id": "@originui/donut-chart",
            "name": "Category Distribution Donut",
            "sourceRegistry": "@originui",
            "category": "Charts",
            "description": "Circular donut chart visualizing market share breakdown and legend labels.",
            "tags": ["donut", "pie", "chart", "analytics", "distribution", "originui"]
        }),
        json!({
            "id": "@kibo/kanban-board",
            "name": "Multi-Column Kanban Board",
            "sourceRegistry": "@kibo",
            "category": "Kanban/Complex",
            "description": "Agile project tracking board with columns (To Do, In Progress, Done) and draggable task cards.",
            "tags": ["kanban", "board", "tasks", "agile", "project", "kibo"]
        }),
        json!({
            "id": "@bundui/data-table",
            "name": "Interactive Data Table",
            "sourceRegistry": "@bundui",
            "category": "Kanban/Complex",
            "description": "High-density enterprise table layout with sorting headers, user avatars, and status badges.",
            "tags": ["table", "data", "grid", "bundui", "complex", "enterprise"]
        }),
        json!({
            "id": "@animate-ui/marquee-ticker",
            "name": "Infinite Marquee Ticker Ribbon",
            "sourceRegistry": "@animate-ui",
            "category": "Animations",
            "description": "Continuous horizontal scrolling banner ribbon for partner logos, metrics, or announcements.",
            "tags": ["marquee", "animation", "ticker", "ribbon", "animate-ui"]
        }),
        json!({
            "id": "@animate-ui/spotlight-card",
            "name": "Cursor Tracking Spotlight Card",
            "sourceRegistry": "@animate-ui",
            "category": "Animations",
            "description": "Card with dynamic radial gradient spotlight that smoothly follows mouse movement.",
            "tags": ["spotlight", "animation", "cursor", "gradient", "animate-ui"]
        }),
    ];

    let matches: Vec<Value> = catalog
        .into_iter()
        .filter(|item| {
            if category != "all" {
                let cat = item.get("category").and_then(Value::as_str).unwrap_or("").to_lowercase();
                if cat != category {
                    return false;
                }
            }
            if registry != "all" {
                let reg = item.get("sourceRegistry").and_then(Value::as_str).unwrap_or("").to_lowercase();
                if reg != registry {
                    return false;
                }
            }
            if !query.is_empty() {
                let name = item.get("name").and_then(Value::as_str).unwrap_or("").to_lowercase();
                let desc = item.get("description").and_then(Value::as_str).unwrap_or("").to_lowercase();
                let id = item.get("id").and_then(Value::as_str).unwrap_or("").to_lowercase();
                let in_tags = item
                    .get("tags")
                    .and_then(Value::as_array)
                    .map(|arr| arr.iter().any(|t| t.as_str().unwrap_or("").to_lowercase().contains(&query)))
                    .unwrap_or(false);
                return name.contains(&query) || desc.contains(&query) || id.contains(&query) || in_tags;
            }
            true
        })
        .collect();

    Ok(json!(matches))
}

fn add_child_frame(s: &mut crate::Session, parent: &Value, name: &str, w: f64, h: f64, props: Value) -> Option<Value> {
    let params = json!({
        "type": "frame",
        "name": name,
        "width": w,
        "height": h,
        "parent": parent,
        "props": props
    });
    s.execute_as(Some("AI"), "node.create", &params).ok()
}

fn add_child_text(
    s: &mut crate::Session,
    parent: &Value,
    name: &str,
    text: &str,
    font_size: f64,
    font_weight: i64,
    color: &str,
) -> Option<Value> {
    let params = json!({
        "type": "text",
        "name": name,
        "parent": parent,
        "text": text,
        "characters": text,
        "props": {
            "fontSize": font_size,
            "fontWeight": font_weight,
            "fills": [{"type": "solid", "color": color}]
        }
    });
    s.execute_as(Some("AI"), "node.create", &params).ok()
}

fn add_child_ellipse(
    s: &mut crate::Session,
    parent: &Value,
    name: &str,
    size: f64,
    stroke_color: &str,
    stroke_width: f64,
    fill_color: &str,
) -> Option<Value> {
    let params = json!({
        "type": "ellipse",
        "name": name,
        "parent": parent,
        "width": size,
        "height": size,
        "props": {
            "fills": [{"type": "solid", "color": fill_color}],
            "strokes": [{"type": "solid", "color": stroke_color, "width": stroke_width}]
        }
    });
    s.execute_as(Some("AI"), "node.create", &params).ok()
}

fn build_button_children(s: &mut crate::Session, r_id: &Value, comp_id: &str, get_text: &dyn Fn(&str, &str) -> String) {
    if comp_id == "@originui/button-shimmer" || comp_id.contains("shimmer") {
        let _ = add_child_text(s, r_id, "Icon", "✦", 13.0, 700, "#60a5fa");
        let label = get_text("Label", "Explore Features");
        let _ = add_child_text(s, r_id, "Label", &label, 14.0, 600, "#93c5fd");
    } else if comp_id == "@neobrutalism/neo-button" || comp_id.contains("neo") {
        let _ = add_child_text(s, r_id, "Icon", "⚡", 13.0, 700, "#000000");
        let label = get_text("Label", "NEO ACTION");
        let _ = add_child_text(s, r_id, "Label", &label, 13.0, 800, "#000000");
    } else if comp_id == "@cult/glow-button" || comp_id.contains("glow") {
        let _ = add_child_text(s, r_id, "Icon", "✨", 13.0, 700, "#f472b6");
        let label = get_text("Label", "Launch Mission →");
        let _ = add_child_text(s, r_id, "Label", &label, 14.0, 600, "#f472b6");
    } else {
        let label = get_text("Label", "Get Started");
        let _ = add_child_text(s, r_id, "Label", &label, 14.0, 600, "#ffffff");
        let _ = add_child_text(s, r_id, "Arrow", "→", 14.0, 600, "#a1a1aa");
    }
}

fn build_input_children(s: &mut crate::Session, r_id: &Value, comp_id: &str, get_text: &dyn Fn(&str, &str) -> String) {
    if comp_id == "@originui/input-search-kbd" || comp_id.contains("search") {
        let _ = add_child_text(s, r_id, "Search Icon", "🔍", 13.0, 400, "#64748b");
        let ph = get_text("Placeholder", "Search components...");
        let _ = add_child_text(s, r_id, "Placeholder", &ph, 13.0, 400, "#94a3b8");
        let badge = add_child_frame(
            s,
            r_id,
            "Badge ⌘K",
            32.0,
            22.0,
            json!({"layout": "horizontal", "cornerRadius": 4.0, "fills": [{"type": "solid", "color": "#f1f5f9"}], "strokes": [{"type": "solid", "color": "#cbd5e1", "width": 1.0}], "padding": [2.0, 6.0, 2.0, 6.0]}),
        );
        if let Some(b) = badge.and_then(|v| v.get("id").cloned()) {
            let _ = add_child_text(s, &b, "Kbd", "⌘K", 11.0, 600, "#64748b");
        }
    } else if comp_id == "@kibo/tags-input" || comp_id.contains("tag") {
        let tag1 = add_child_frame(
            s,
            r_id,
            "Tag / React",
            68.0,
            28.0,
            json!({"cornerRadius": 6.0, "fills": [{"type": "solid", "color": "#f3e8ff"}], "padding": [4.0, 8.0, 4.0, 8.0]}),
        );
        if let Some(t) = tag1.and_then(|v| v.get("id").cloned()) {
            let _ = add_child_text(s, &t, "Tag Text", "React ✕", 12.0, 600, "#7e22ce");
        }
        let tag2 = add_child_frame(
            s,
            r_id,
            "Tag / Tailwind",
            82.0,
            28.0,
            json!({"cornerRadius": 6.0, "fills": [{"type": "solid", "color": "#e0e7ff"}], "padding": [4.0, 8.0, 4.0, 8.0]}),
        );
        if let Some(t) = tag2.and_then(|v| v.get("id").cloned()) {
            let _ = add_child_text(s, &t, "Tag Text", "Tailwind ✕", 12.0, 600, "#4338ca");
        }
        let _ = add_child_text(s, r_id, "Input Prompt", "Add tag...", 12.0, 400, "#94a3b8");
    } else {
        let ph = get_text("Placeholder", "Enter email address...");
        let _ = add_child_text(s, r_id, "Placeholder", &ph, 13.0, 400, "#94a3b8");
    }
}

fn build_card_children(s: &mut crate::Session, r_id: &Value, comp_id: &str, get_text: &dyn Fn(&str, &str) -> String) {
    if comp_id == "@originui/pricing-card" || comp_id.contains("pricing") {
        let br = add_child_frame(s, r_id, "Badge Row", 240.0, 24.0, json!({"layout": "horizontal", "itemSpacing": 8.0}));
        if let Some(b_row) = br.and_then(|v| v.get("id").cloned()) {
            let plan = get_text("Plan", "Pro Plan");
            let _ = add_child_text(s, &b_row, "Plan Name", &plan, 18.0, 700, "#0f172a");
            let b_pill = add_child_frame(
                s,
                &b_row,
                "Badge",
                96.0,
                22.0,
                json!({"cornerRadius": 10.0, "fills": [{"type": "solid", "color": "#4f46e5"}], "padding": [3.0, 8.0, 3.0, 8.0]}),
            );
            if let Some(bp) = b_pill.and_then(|v| v.get("id").cloned()) {
                let _ = add_child_text(s, &bp, "Popular", "MOST POPULAR", 10.0, 700, "#ffffff");
            }
        }
        let price = get_text("Price", "$29 / month");
        let _ = add_child_text(s, r_id, "Price", &price, 30.0, 800, "#0f172a");
        let _ = add_child_text(s, r_id, "Feature 1", "✓ Unlimited canvas layers & assets", 12.0, 500, "#475569");
        let _ = add_child_text(s, r_id, "Feature 2", "✓ Real-time collaboration & sync", 12.0, 500, "#475569");
        let _ = add_child_text(s, r_id, "Feature 3", "✓ Priority AI market component solver", 12.0, 500, "#475569");
        let cta = add_child_frame(
            s,
            r_id,
            "CTA Button",
            240.0,
            40.0,
            json!({"layout": "horizontal", "cornerRadius": 10.0, "fills": [{"type": "solid", "color": "#4f46e5"}], "padding": [10.0, 18.0, 10.0, 18.0]}),
        );
        if let Some(c) = cta.and_then(|v| v.get("id").cloned()) {
            let cta_text = get_text("CTA", "Get Started Now");
            let _ = add_child_text(s, &c, "Button Label", &cta_text, 14.0, 600, "#ffffff");
        }
    } else if comp_id == "@cult/bento-card" || comp_id.contains("bento") {
        let ib = add_child_frame(
            s,
            r_id,
            "Icon Box",
            44.0,
            44.0,
            json!({"cornerRadius": 10.0, "fills": [{"type": "solid", "color": "#312e81"}], "strokes": [{"type": "solid", "color": "#4338ca", "width": 1.0}], "padding": [10.0, 10.0, 10.0, 10.0]}),
        );
        if let Some(i) = ib.and_then(|v| v.get("id").cloned()) {
            let _ = add_child_text(s, &i, "Icon Symbol", "✦", 18.0, 400, "#a5b4fc");
        }
        let heading = get_text("Heading", "Intelligent AI Layouts");
        let _ = add_child_text(s, r_id, "Heading", &heading, 16.0, 700, "#ffffff");
        let desc = get_text("Description", "Automatic layout engine that aligns and spaces design tokens with precision.");
        let _ = add_child_text(s, r_id, "Description", &desc, 12.0, 400, "#a3a3a3");
    } else {
        let hr = add_child_frame(s, r_id, "Header Row", 220.0, 28.0, json!({"layout": "horizontal", "itemSpacing": 8.0}));
        if let Some(h) = hr.and_then(|v| v.get("id").cloned()) {
            let icon_box = add_child_frame(
                s,
                &h,
                "Icon Badge",
                24.0,
                24.0,
                json!({"cornerRadius": 6.0, "fills": [{"type": "solid", "color": "#eff6ff"}], "strokes": [{"type": "solid", "color": "#dbeafe", "width": 1.0}]}),
            );
            if let Some(ib) = icon_box.and_then(|v| v.get("id").cloned()) {
                let _ = add_child_text(s, &ib, "Icon", "$", 12.0, 700, "#2563eb");
            }
            let title = get_text("Title", "Total Revenue");
            let _ = add_child_text(s, &h, "Title", &title, 13.0, 600, "#64748b");
        }
        let value = get_text("Value", "$48,250.00");
        let _ = add_child_text(s, r_id, "Metric Value", &value, 28.0, 800, "#0f172a");
        let tr = add_child_frame(s, r_id, "Trend Row", 220.0, 24.0, json!({"layout": "horizontal", "itemSpacing": 8.0}));
        if let Some(t) = tr.and_then(|v| v.get("id").cloned()) {
            let pill = add_child_frame(
                s,
                &t,
                "Trend Pill",
                68.0,
                20.0,
                json!({"cornerRadius": 10.0, "fills": [{"type": "solid", "color": "#dcfce7"}], "padding": [2.0, 6.0, 2.0, 6.0]}),
            );
            if let Some(p) = pill.and_then(|v| v.get("id").cloned()) {
                let _ = add_child_text(s, &p, "Trend", "+18.4% ↑", 10.0, 700, "#15803d");
            }
            let _ = add_child_text(s, &t, "Period", "vs last month", 11.0, 400, "#94a3b8");
        }
    }
}

fn build_chart_children(s: &mut crate::Session, r_id: &Value, comp_id: &str, get_text: &dyn Fn(&str, &str) -> String) {
    if comp_id == "@originui/donut-chart" || comp_id.contains("donut") {
        let lc = add_child_frame(s, r_id, "Labels Column", 140.0, 90.0, json!({"layout": "vertical", "itemSpacing": 6.0}));
        if let Some(l) = lc.and_then(|v| v.get("id").cloned()) {
            let _ = add_child_text(s, &l, "Title", "Traffic Sources", 14.0, 700, "#0f172a");
            let _ = add_child_text(s, &l, "Direct", "● Direct: 65%", 12.0, 500, "#4f46e5");
            let _ = add_child_text(s, &l, "Social", "● Organic: 35%", 12.0, 500, "#10b981");
        }
        let _ = add_child_ellipse(s, r_id, "Donut Circle", 70.0, "#4f46e5", 8.0, "#e0e7ff");
    } else {
        let title = get_text("Title", "Revenue Growth Overview");
        let _ = add_child_text(s, r_id, "Chart Title", &title, 15.0, 700, "#0f172a");
        let bc = add_child_frame(
            s,
            r_id,
            "Chart Bars Container",
            280.0,
            100.0,
            json!({"layout": "horizontal", "itemSpacing": 14.0, "padding": [10.0, 10.0, 0.0, 10.0]}),
        );
        if let Some(b) = bc.and_then(|v| v.get("id").cloned()) {
            let bars = [("Bar Jan", 45.0, "#818cf8"), ("Bar Feb", 60.0, "#6366f1"), ("Bar Mar", 75.0, "#4f46e5"), ("Bar Apr", 70.0, "#4338ca"), ("Bar May", 95.0, "#3730a3")];
            for (bn, bh, bc_col) in bars {
                let _ = add_child_frame(s, &b, bn, 28.0, bh, json!({"cornerRadius": 4.0, "fills": [{"type": "solid", "color": bc_col}]}));
            }
        }
    }
}

fn build_complex_children(s: &mut crate::Session, r_id: &Value, comp_id: &str) {
    if comp_id == "@bundui/data-table" || comp_id.contains("table") {
        let th = add_child_frame(
            s,
            r_id,
            "Table Header",
            380.0,
            36.0,
            json!({"layout": "horizontal", "fills": [{"type": "solid", "color": "#f8fafc"}], "padding": [10.0, 16.0, 10.0, 16.0]}),
        );
        if let Some(h) = th.and_then(|v| v.get("id").cloned()) {
            let _ = add_child_text(s, &h, "H User", "NAME / USER", 11.0, 700, "#64748b");
            let _ = add_child_text(s, &h, "H Role", "ROLE", 11.0, 700, "#64748b");
            let _ = add_child_text(s, &h, "H Status", "STATUS", 11.0, 700, "#64748b");
        }
        let tr = add_child_frame(
            s,
            r_id,
            "Row 1",
            380.0,
            48.0,
            json!({"layout": "horizontal", "padding": [12.0, 16.0, 12.0, 16.0]}),
        );
        if let Some(r) = tr.and_then(|v| v.get("id").cloned()) {
            let _ = add_child_text(s, &r, "User 1", "Sarah Chen", 13.0, 600, "#0f172a");
            let _ = add_child_text(s, &r, "Role 1", "Lead Designer", 12.0, 400, "#475569");
            let badge = add_child_frame(
                s,
                &r,
                "Badge Active",
                56.0,
                22.0,
                json!({"cornerRadius": 10.0, "fills": [{"type": "solid", "color": "#d1fae5"}]}),
            );
            if let Some(b) = badge.and_then(|v| v.get("id").cloned()) {
                let _ = add_child_text(s, &b, "Status", "ACTIVE", 10.0, 700, "#065f46");
            }
        }
    } else {
        let cols = [
            ("Column / To Do", "📋 TO DO (3)", "#475569", "Card 1", "Design Market UI", "#f1f5f9", "#334155"),
            ("Column / In Progress", "⚡ IN PROGRESS (2)", "#f59e0b", "Card 2", "AI Market Priority", "#fef3c7", "#b45309"),
            ("Column / Done", "✓ DONE (5)", "#10b981", "Card 3", "Shadcn Registries", "#d1fae5", "#047857"),
        ];
        for (c_name, c_hdr, hdr_color, card_name, task_title, card_bg, task_color) in cols {
            let col = add_child_frame(
                s,
                r_id,
                c_name,
                136.0,
                220.0,
                json!({"layout": "vertical", "itemSpacing": 10.0, "padding": [12.0, 12.0, 12.0, 12.0], "cornerRadius": 10.0, "fills": [{"type": "solid", "color": "#ffffff"}], "strokes": [{"type": "solid", "color": "#e2e8f0", "width": 1.0}]}),
            );
            if let Some(cf) = col.and_then(|v| v.get("id").cloned()) {
                let _ = add_child_text(s, &cf, "Col Header", c_hdr, 12.0, 700, hdr_color);
                let card = add_child_frame(
                    s,
                    &cf,
                    card_name,
                    112.0,
                    50.0,
                    json!({"cornerRadius": 6.0, "fills": [{"type": "solid", "color": card_bg}], "padding": [8.0, 8.0, 8.0, 8.0]}),
                );
                if let Some(cd) = card.and_then(|v| v.get("id").cloned()) {
                    let _ = add_child_text(s, &cd, "Task", task_title, 11.0, 500, task_color);
                }
            }
        }
    }
}

fn build_animation_children(s: &mut crate::Session, r_id: &Value, comp_id: &str, get_text: &dyn Fn(&str, &str) -> String) {
    if comp_id == "@animate-ui/marquee-ticker" || comp_id.contains("marquee") {
        let _ = add_child_text(s, r_id, "Item 1", "⚡ 100% Free Registries", 13.0, 700, "#34d399");
        let _ = add_child_text(s, r_id, "Item 2", "✦ Shadcn Ecosystem", 13.0, 600, "#ffffff");
        let _ = add_child_text(s, r_id, "Item 3", "🚀 Instant Canvas Drop", 13.0, 600, "#60a5fa");
    } else {
        let title = get_text("Title", "Interactive Spotlight");
        let sub = get_text("Subtitle", "Smooth radial focus tracking your pointer across canvas elements.");
        let _ = add_child_text(s, r_id, "Title", &title, 15.0, 700, "#f8fafc");
        let _ = add_child_text(s, r_id, "Subtitle", &sub, 12.0, 400, "#94a3b8");
    }
}

struct MarketComponentSpec {
    name: &'static str,
    width: f64,
    height: f64,
    fills: Value,
    corner_radius: f64,
    stroke: Option<&'static str>,
    layout_mode: &'static str,
    item_spacing: f64,
    padding: Value,
    effects: Option<Value>,
}

fn market_component_spec(comp_id: &str) -> MarketComponentSpec {
    if comp_id == "@shadcn/button"
        || (comp_id.contains("button")
            && !comp_id.contains("shimmer")
            && !comp_id.contains("neo")
            && !comp_id.contains("glow"))
    {
        MarketComponentSpec {
            name: "Button / Primary",
            width: 140.0,
            height: 42.0,
            fills: json!([{"type": "solid", "color": "#18181b"}]),
            corner_radius: 8.0,
            stroke: Some("#27272a"),
            layout_mode: "horizontal",
            item_spacing: 8.0,
            padding: json!([10.0, 18.0, 10.0, 18.0]),
            effects: Some(json!([{"type": "dropShadow", "color": "#00000026", "offset": [0.0, 2.0], "radius": 4.0, "spread": 0.0}])),
        }
    } else if comp_id == "@originui/button-shimmer" || comp_id.contains("shimmer") {
        MarketComponentSpec {
            name: "Button / Shimmer",
            width: 156.0,
            height: 42.0,
            fills: json!([{"type": "solid", "color": "#090d16"}]),
            corner_radius: 999.0,
            stroke: Some("#3b82f6"),
            layout_mode: "horizontal",
            item_spacing: 8.0,
            padding: json!([11.0, 20.0, 11.0, 20.0]),
            effects: Some(json!([{"type": "dropShadow", "color": "#3b82f640", "offset": [0.0, 4.0], "radius": 12.0, "spread": 0.0}])),
        }
    } else if comp_id == "@neobrutalism/neo-button" || comp_id.contains("neo") {
        MarketComponentSpec {
            name: "Button / Neobrutalism",
            width: 140.0,
            height: 44.0,
            fills: json!([{"type": "solid", "color": "#fde047"}]),
            corner_radius: 6.0,
            stroke: Some("#000000"),
            layout_mode: "horizontal",
            item_spacing: 6.0,
            padding: json!([10.0, 16.0, 10.0, 16.0]),
            effects: Some(json!([{"type": "dropShadow", "color": "#000000", "offset": [4.0, 4.0], "radius": 0.0, "spread": 0.0}])),
        }
    } else if comp_id == "@cult/glow-button" || comp_id.contains("glow") {
        MarketComponentSpec {
            name: "Button / Cyber Glow",
            width: 164.0,
            height: 44.0,
            fills: json!([{"type": "solid", "color": "#13041c"}]),
            corner_radius: 10.0,
            stroke: Some("#ec4899"),
            layout_mode: "horizontal",
            item_spacing: 8.0,
            padding: json!([11.0, 20.0, 11.0, 20.0]),
            effects: Some(json!([{"type": "dropShadow", "color": "#ec489980", "offset": [0.0, 0.0], "radius": 18.0, "spread": 2.0}])),
        }
    } else if comp_id == "@shadcn/input"
        || (comp_id.contains("input") && !comp_id.contains("search") && !comp_id.contains("tag"))
    {
        MarketComponentSpec {
            name: "Input / Text Field",
            width: 240.0,
            height: 42.0,
            fills: json!([{"type": "solid", "color": "#ffffff"}]),
            corner_radius: 8.0,
            stroke: Some("#cbd5e1"),
            layout_mode: "horizontal",
            item_spacing: 8.0,
            padding: json!([10.0, 14.0, 10.0, 14.0]),
            effects: Some(json!([{"type": "dropShadow", "color": "#00000008", "offset": [0.0, 1.0], "radius": 2.0, "spread": 0.0}])),
        }
    } else if comp_id == "@originui/input-search-kbd" || comp_id.contains("search") {
        MarketComponentSpec {
            name: "Input / Search with ⌘K",
            width: 280.0,
            height: 44.0,
            fills: json!([{"type": "solid", "color": "#ffffff"}]),
            corner_radius: 8.0,
            stroke: Some("#e2e8f0"),
            layout_mode: "horizontal",
            item_spacing: 10.0,
            padding: json!([10.0, 14.0, 10.0, 14.0]),
            effects: Some(json!([{"type": "dropShadow", "color": "#0000000c", "offset": [0.0, 2.0], "radius": 4.0, "spread": 0.0}])),
        }
    } else if comp_id == "@kibo/tags-input" || comp_id.contains("tag") {
        MarketComponentSpec {
            name: "Input / Tag Chips",
            width: 280.0,
            height: 48.0,
            fills: json!([{"type": "solid", "color": "#ffffff"}]),
            corner_radius: 8.0,
            stroke: Some("#e2e8f0"),
            layout_mode: "horizontal",
            item_spacing: 8.0,
            padding: json!([8.0, 12.0, 8.0, 12.0]),
            effects: Some(json!([{"type": "dropShadow", "color": "#0000000c", "offset": [0.0, 2.0], "radius": 4.0, "spread": 0.0}])),
        }
    } else if comp_id == "@shadcn/card-metric" || comp_id.contains("metric") {
        MarketComponentSpec {
            name: "Card / Metric KPI",
            width: 260.0,
            height: 140.0,
            fills: json!([{"type": "solid", "color": "#ffffff"}]),
            corner_radius: 14.0,
            stroke: Some("#e2e8f0"),
            layout_mode: "vertical",
            item_spacing: 10.0,
            padding: json!([18.0, 20.0, 18.0, 20.0]),
            effects: Some(json!([{"type": "dropShadow", "color": "#00000012", "offset": [0.0, 6.0], "radius": 16.0, "spread": -2.0}])),
        }
    } else if comp_id == "@originui/pricing-card" || comp_id.contains("pricing") {
        MarketComponentSpec {
            name: "Card / Pricing Highlight",
            width: 280.0,
            height: 320.0,
            fills: json!([{"type": "solid", "color": "#ffffff"}]),
            corner_radius: 16.0,
            stroke: Some("#4f46e5"),
            layout_mode: "vertical",
            item_spacing: 12.0,
            padding: json!([20.0, 20.0, 20.0, 20.0]),
            effects: Some(json!([{"type": "dropShadow", "color": "#4f46e522", "offset": [0.0, 10.0], "radius": 26.0, "spread": -4.0}])),
        }
    } else if comp_id == "@cult/bento-card" || comp_id.contains("bento") {
        MarketComponentSpec {
            name: "Card / Bento Showcase",
            width: 280.0,
            height: 200.0,
            fills: json!([{"type": "solid", "color": "#171717"}]),
            corner_radius: 16.0,
            stroke: Some("#2e2e2e"),
            layout_mode: "vertical",
            item_spacing: 12.0,
            padding: json!([22.0, 22.0, 22.0, 22.0]),
            effects: Some(json!([{"type": "dropShadow", "color": "#00000060", "offset": [0.0, 12.0], "radius": 28.0, "spread": -4.0}])),
        }
    } else if comp_id == "@shadcn/area-chart" || comp_id.contains("area") {
        MarketComponentSpec {
            name: "Chart / Revenue Area Curve",
            width: 320.0,
            height: 190.0,
            fills: json!([{"type": "solid", "color": "#ffffff"}]),
            corner_radius: 14.0,
            stroke: Some("#e2e8f0"),
            layout_mode: "vertical",
            item_spacing: 12.0,
            padding: json!([16.0, 20.0, 16.0, 20.0]),
            effects: Some(json!([{"type": "dropShadow", "color": "#00000010", "offset": [0.0, 4.0], "radius": 12.0, "spread": -2.0}])),
        }
    } else if comp_id == "@originui/donut-chart" || comp_id.contains("donut") {
        MarketComponentSpec {
            name: "Chart / Donut Distribution",
            width: 260.0,
            height: 140.0,
            fills: json!([{"type": "solid", "color": "#ffffff"}]),
            corner_radius: 14.0,
            stroke: Some("#e2e8f0"),
            layout_mode: "horizontal",
            item_spacing: 14.0,
            padding: json!([16.0, 20.0, 16.0, 20.0]),
            effects: Some(json!([{"type": "dropShadow", "color": "#00000010", "offset": [0.0, 4.0], "radius": 12.0, "spread": -2.0}])),
        }
    } else if comp_id == "@kibo/kanban-board" || comp_id.contains("kanban") {
        MarketComponentSpec {
            name: "Kanban / Sprint Board",
            width: 480.0,
            height: 270.0,
            fills: json!([{"type": "solid", "color": "#f8fafc"}]),
            corner_radius: 16.0,
            stroke: Some("#e2e8f0"),
            layout_mode: "horizontal",
            item_spacing: 16.0,
            padding: json!([16.0, 16.0, 16.0, 16.0]),
            effects: Some(json!([{"type": "dropShadow", "color": "#0000000e", "offset": [0.0, 8.0], "radius": 24.0, "spread": -4.0}])),
        }
    } else if comp_id == "@bundui/data-table" || comp_id.contains("table") {
        MarketComponentSpec {
            name: "Table / Data Grid",
            width: 380.0,
            height: 170.0,
            fills: json!([{"type": "solid", "color": "#ffffff"}]),
            corner_radius: 12.0,
            stroke: Some("#e2e8f0"),
            layout_mode: "vertical",
            item_spacing: 0.0,
            padding: json!([0.0, 0.0, 0.0, 0.0]),
            effects: Some(json!([{"type": "dropShadow", "color": "#00000010", "offset": [0.0, 4.0], "radius": 14.0, "spread": -2.0}])),
        }
    } else if comp_id == "@animate-ui/marquee-ticker" || comp_id.contains("marquee") {
        MarketComponentSpec {
            name: "Animation / Marquee Banner",
            width: 360.0,
            height: 46.0,
            fills: json!([{"type": "solid", "color": "#171717"}]),
            corner_radius: 10.0,
            stroke: Some("#262626"),
            layout_mode: "horizontal",
            item_spacing: 18.0,
            padding: json!([12.0, 18.0, 12.0, 18.0]),
            effects: Some(json!([{"type": "dropShadow", "color": "#00000040", "offset": [0.0, 4.0], "radius": 10.0, "spread": 0.0}])),
        }
    } else if comp_id == "@animate-ui/spotlight-card" || comp_id.contains("spotlight") {
        MarketComponentSpec {
            name: "Animation / Spotlight Card",
            width: 260.0,
            height: 150.0,
            fills: json!([{"type": "solid", "color": "#0f172a"}]),
            corner_radius: 16.0,
            stroke: Some("#334155"),
            layout_mode: "vertical",
            item_spacing: 10.0,
            padding: json!([18.0, 18.0, 18.0, 18.0]),
            effects: Some(json!([{"type": "dropShadow", "color": "#0284c728", "offset": [0.0, 8.0], "radius": 24.0, "spread": -2.0}])),
        }
    } else {
        MarketComponentSpec {
            name: "Market / Component",
            width: 260.0,
            height: 140.0,
            fills: json!([{"type": "solid", "color": "#ffffff"}]),
            corner_radius: 12.0,
            stroke: Some("#e2e8f0"),
            layout_mode: "vertical",
            item_spacing: 8.0,
            padding: json!([16.0, 16.0, 16.0, 16.0]),
            effects: Some(json!([{"type": "dropShadow", "color": "#00000010", "offset": [0.0, 4.0], "radius": 12.0, "spread": -2.0}])),
        }
    }
}

/// Place a prebuilt Market component directly onto the canvas with high-fidelity styling and sub-components.
pub fn instantiate_market_component(
    s: &mut crate::Session,
    comp_id: &str,
    x: f64,
    y: f64,
    parent: Option<&str>,
    custom_text: Option<&serde_json::Map<String, Value>>,
) -> Result<Value, String> {
    let get_text = |key: &str, fallback: &str| -> String {
        custom_text
            .and_then(|ct| ct.get(key))
            .and_then(Value::as_str)
            .unwrap_or(fallback)
            .to_string()
    };

    let spec = market_component_spec(comp_id);

    let mut frame_props = json!({
        "layout": spec.layout_mode,
        "layoutMode": spec.layout_mode,
        "itemSpacing": spec.item_spacing,
        "padding": spec.padding,
        "cornerRadius": spec.corner_radius,
        "fills": spec.fills,
    });

    if let Some(st) = spec.stroke {
        let width = if comp_id.contains("neo") || comp_id.contains("pricing") { 2.0 } else { 1.5 };
        frame_props["strokes"] = json!([{"type": "solid", "color": st, "width": width}]);
    }

    if let Some(eff) = spec.effects {
        frame_props["effects"] = eff;
    }

    let mut create_params = json!({
        "type": "frame",
        "name": spec.name,
        "x": x,
        "y": y,
        "width": spec.width,
        "height": spec.height,
        "props": frame_props
    });

    if let (Some(p), Some(obj)) = (parent, create_params.as_object_mut()) {
        obj.insert("parent".into(), json!(p));
    }

    let root = s.execute_as(Some("AI"), "node.create", &create_params).map_err(|e| e.to_string())?;
    let root_id = root.get("id").cloned();

    if let Some(r_id) = &root_id {
        if comp_id.contains("button") {
            build_button_children(s, r_id, comp_id, &get_text);
        } else if comp_id.contains("input") || comp_id.contains("search") || comp_id.contains("tag") {
            build_input_children(s, r_id, comp_id, &get_text);
        } else if comp_id.contains("metric") || comp_id.contains("pricing") || comp_id.contains("bento") {
            build_card_children(s, r_id, comp_id, &get_text);
        } else if comp_id.contains("chart") || comp_id.contains("area") || comp_id.contains("donut") {
            build_chart_children(s, r_id, comp_id, &get_text);
        } else if comp_id.contains("kanban") || comp_id.contains("table") {
            build_complex_children(s, r_id, comp_id);
        } else if comp_id.contains("marquee") || comp_id.contains("spotlight") {
            build_animation_children(s, r_id, comp_id, &get_text);
        } else {
            let title = get_text("Title", &format!("Component {comp_id}"));
            let _ = add_child_text(s, r_id, "Title", &title, 14.0, 700, "#0f172a");
            let _ = add_child_text(s, r_id, "Desc", "Shadcn ecosystem prebuilt element ready to customize.", 12.0, 400, "#64748b");
        }
    }

    Ok(json!({
        "id": root_id,
        "component_id": comp_id,
        "status": "inserted",
        "name": spec.name,
        "x": x,
        "y": y,
        "width": spec.width,
        "height": spec.height
    }))
}

/// Place a prebuilt Market component directly onto the canvas with customization.
fn add_market_component_to_canvas(s: &mut crate::Session, arguments: &Value) -> Result<Value, String> {
    let comp_id = arguments.get("component_id").and_then(Value::as_str).unwrap_or_default().trim();
    if comp_id.is_empty() {
        return Err("`component_id` is required".into());
    }

    let parent =
        arguments.get("parent_frame_id").and_then(|v| if let Some(i) = v.as_i64() { Some(i.to_string()) } else { v.as_str().map(str::to_string) });

    let (mut x, mut y) = (160.0, 120.0);
    if let Some(pos) = arguments.get("position") {
        x = pos.get("x").and_then(Value::as_f64).unwrap_or(x);
        y = pos.get("y").and_then(Value::as_f64).unwrap_or(y);
    }

    let custom_text = arguments.get("custom_text").and_then(Value::as_object);

    instantiate_market_component(s, comp_id, x, y, parent.as_deref(), custom_text)
}

/// `list_commands`' answer: one compact line per command, narrowed by `filter`.
///
/// The catalogue is over 700 commands and about 190 KB of text. An answer that size crowds a
/// model's context until it answers with nothing, so with no filter this is an index — the ids
/// grouped by menu — and a filter gives the full lines, longest answer still a few KB. The
/// filter matches an id, label, menu or parameter doc, as the MCP server's `list_commands` does.
fn list_commands(arguments: &Value) -> String {
    let filter = arguments.get("filter").and_then(Value::as_str).unwrap_or_default().trim().to_lowercase();
    let all = crate::cmd::commands_json();
    let empty = Vec::new();
    let list = all.as_array().unwrap_or(&empty);
    if list.is_empty() {
        return "the command catalogue is empty".into();
    }
    if filter.is_empty() {
        return command_index(list);
    }
    let mut out = String::new();
    let mut shown = 0usize;
    for c in list {
        let g = |k: &str| c.get(k).and_then(Value::as_str).unwrap_or("");
        if !["id", "label", "menu", "params"].iter().any(|k| g(k).to_lowercase().contains(&filter)) {
            continue;
        }
        shown += 1;
        out.push_str(&command_line(c));
        out.push('\n');
    }
    if shown == 0 {
        // The filter may name a tool rather than a command: "source_image" is a tool, and a model
        // that reads "no command matches" goes looking for another way to do the same job.
        if let Some(tool) = crate::chat_tools::table().iter().find(|t| t.name.contains(&filter)) {
            return format!(
                "`{filter}` is a tool, not a command: call it as `{}`. Its parameters: {}",
                tool.name,
                tool.schema.get("properties").map(|p| p.to_string()).unwrap_or_default()
            );
        }
        return format!(
            "no command matches `{filter}` ({} commands in all). Ids are listed with no filter; narrow with a word from an id, label, menu or parameter doc.",
            list.len()
        );
    }
    format!("{shown} of {} commands matching `{filter}` (id* — label [menu] (shortcut) params; * = undoable):\n{out}", list.len())
}

/// The whole catalogue as an index: one line per menu, the command ids under it. Small enough to
/// read in full, and enough to know what to filter for.
fn command_index(list: &[Value]) -> String {
    let mut by_menu: Vec<(&str, Vec<&str>)> = Vec::new();
    for c in list {
        let menu = c.get("menu").and_then(Value::as_str).unwrap_or("other");
        let id = c.get("id").and_then(Value::as_str).unwrap_or_default();
        match by_menu.iter_mut().find(|(m, _)| *m == menu) {
            Some((_, ids)) => ids.push(id),
            None => by_menu.push((menu, vec![id])),
        }
    }
    let mut out = format!(
        "{} commands in {} menus. This is the index: `list_commands` again with a filter (a word from an id, a label, a menu or a parameter doc) gives the full line with that command's parameters.\n",
        list.len(),
        by_menu.len()
    );
    for (menu, ids) in by_menu {
        out.push_str(&format!("\n{menu}: {}", ids.join(", ")));
    }
    out.push_str("\n\nThe tools you already have (create_node, set_properties, batch, get_node, export) cover drawing and changing; props_reference documents every property they take.");
    out
}

/// One command as a line: `id* — label [menu] (shortcut) params`.
fn command_line(c: &Value) -> String {
    let g = |k: &str| c.get(k).and_then(Value::as_str).unwrap_or("");
    let mut out = String::from(g("id"));
    if c.get("undoable").and_then(Value::as_bool) == Some(true) {
        out.push('*');
    }
    out.push_str(" — ");
    out.push_str(g("label"));
    if !g("menu").is_empty() {
        out.push_str(&format!(" [{}]", g("menu")));
    }
    if !g("shortcut").is_empty() {
        out.push_str(&format!(" ({})", g("shortcut")));
    }
    out.push(' ');
    out.push_str(g("params"));
    out
}

/// `props_reference`'s answer: the `node.set` keys with the exact shapes they accept, so the model
/// writes a call that works instead of discovering the schema by trial and error.
///
/// `KEY` narrows it to entries whose name contains it. The text is documentation for the model,
/// not for people: every line is copy-and-paste ready.
fn props_reference(arguments: &Value) -> String {
    let only = arguments.get("key").and_then(Value::as_str).unwrap_or_default().to_lowercase();
    let wanted = |keys: &[&str]| only.is_empty() || keys.iter().any(|k| k.to_lowercase().contains(&only));
    let mut out = String::from(
        "node.set props. Every value below is accepted exactly as written. Colours are \"#rrggbb\", {\"r\",\"g\",\"b\" (0-1)}, [r,g,b] or a name.\n",
    );
    if wanted(&["fills", "fill", "strokes", "stroke", "opacity", "blend"]) {
        out.push_str(
            "\
fills: [\"#ff0000\"] | [{\"type\":\"solid\",\"color\":{\"r\":1,\"g\":0,\"b\":0}}] | [{\"type\":\"linearGradient\",\"stops\":[\"#000\",\"#fff\"],\"from\":[0,0],\"to\":[1,0]}]
  gradients: linearGradient | radialGradient | angularGradient | diamondGradient, with \"stops\" as colours or {\"color\",\"position\" (0-1)}
  images: {\"type\":\"image\",\"src\":\"<url>\"} or {\"type\":\"image\",\"data\":\"<base64 png>\"}
fill: \"#ff0000\" | {$var: \"color/primary\"} | \"none\" | null      (one paint)
strokes: same shapes as fills. stroke: a colour, a {$var}, \"none\" or null. strokeWeight: 1. strokeAlign: inside|center|outside.
opacity: 0-1 or \"50%\". blend: normal|multiply|screen|overlay|darken|lighten|…
",
        );
    }
    if wanted(&["layout", "gap", "padding", "mainAlign", "crossAlign", "wrap", "grid"]) {
        out.push_str(
            "\
layout: \"vertical\" | \"horizontal\" | \"grid\" | {\"mode\":\"vertical\", \"itemSpacing\":16, \"padding\":24} | null
  in the object: mode | itemSpacing (also \"gap\") | counterAxisSpacing (also \"crossGap\") | padding | paddingTop/Right/Bottom/Left
  | primaryAxisAlignItems (min|center|max|spaceBetween) | counterAxisAlignItems (min|center|max|baseline) | wrap
  padding: 24 (all sides) | [top,right,bottom,left] | {\"top\":24,\"right\":32,\"bottom\":24,\"left\":32}
  layoutPaddingTop/Right/Bottom/Left/Horizontal/Vertical work too, in `layout` or beside it.
gap: 16 (sets itemSpacing, needs a layout). crossGap: 12 (sets counterAxisSpacing).
mainAlign: min|center|max|spaceBetween|spaceAround|spaceEvenly. crossAlign: min|center|max|baseline. wrap: true.
grid: layout \"grid\" with gridColumns: 3, gridRows: 2, columnTracks: [\"1fr\",\"1fr\",\"1fr\"], rowTracks: [120,\"auto\"].
  grid children: gridColumn / gridRow (0-based), gridColumnSpan, gridRowSpan, gridAlignH / gridAlignV (min|center|max).
",
        );
    }
    if wanted(&["sizing", "hug", "fill", "minWidth", "maxWidth", "absolute"]) {
        out.push_str(
            "\
sizingH / sizingV: fixed | hug | fill        (these, not \"sizingHorizontal\"/\"sizingVertical\")
  also layoutSizingHorizontal / layoutSizingVertical. fill needs an auto layout parent; hug needs a layout or text.
  A wrapped text usually wants sizingH \"fill\" (or a width) — a fixed width guesses the font metrics and spills.
absolute: true (out of the flow). minWidth / maxWidth / minHeight / maxHeight: numbers, null clears.
",
        );
    }
    if wanted(&["corner", "radius", "clip", "mask", "effect", "shadow", "blur"]) {
        out.push_str(
            "\
cornerRadius: 8 | [topLeft,topRight,bottomRight,bottomLeft] | {\"topLeft\":8,\"topRight\":8}
topLeftRadius / topRightRadius / bottomRightRadius / bottomLeftRadius: 8. cornerSmoothing: 0-1.
clip: true. isMask: true. visible: false. locked: true. name: \"Hero\". description: \"…\".
effects: [{\"type\":\"dropShadow\",\"color\":\"#00000026\",\"x\":0,\"y\":4,\"blur\":12,\"spread\":0}|{\"type\":\"innerShadow\",…}|{\"type\":\"layerBlur\",\"radius\":4}|{\"type\":\"backgroundBlur\",\"radius\":8}]
",
        );
    }
    if wanted(&["text", "font", "character", "lineheight", "letterspacing", "align", "wrap"]) {
        out.push_str(
            "\
characters: \"Hello\" (or text). fontFamily: \"Inter\" (or any family name, e.g. \"Playfair Display\", \"Roboto\"). fontStyle: \"Regular\"|\"Medium\"|\"Semi Bold\"|\"Bold\"|\"Italic\".
fontSize: 16. lineHeight: \"auto\" | 24 | \"150%\". letterSpacing: 0 | \"2%\". textAlign: left|center|right|justified.
textAlignVertical: top|center|bottom. textAutoResize: widthAndHeight (hugs) | height (fixed width, wraps) | none | truncate.
textCase: original|upper|lower|title. maxLines: 2. paragraphSpacing: 8.
(Fonts can be loaded by family name via text.loadFont with `family`, or directly specified on text layers via `fontFamily`.)
",
        );
    }
    if wanted(&["x", "y", "width", "height", "rotation", "geometry", "path"]) {
        out.push_str(
            "\
x / y: position in the parent frame. width / height: size. rotation: degrees. absoluteRotation: degrees on the page.
path: SVG path data (a vector's geometry). pointCount / innerRadius: polygon and star.
",
        );
    }
    if wanted(&["variable", "$var", "bind"]) {
        out.push_str(
            "\
A design token anywhere a value goes: {$var: \"color/primary\"} (a variable id or name). Bindings are separate from values.
",
        );
    }
    out.push_str("Unknown keys are refused by name. Other commands: `list_commands` with a filter.\n");
    out
}

/// `inspect_document`'s answer: the document as the inspect module reads it.
fn inspect(s: &mut crate::Session, arguments: &Value) -> Result<Value, String> {
    let depth = arguments.get("depth").and_then(Value::as_u64).unwrap_or(8).min(64) as usize;
    let options = crate::automation::inspect::InspectOptions::parse(arguments).map_err(|e| e.to_string())?;
    let all_pages = arguments.get("allPages").and_then(Value::as_bool).unwrap_or(false);
    s.doc().map_err(|e| e.to_string()).map(|_| crate::automation::inspect::document_with_options(s, depth, all_pages, &options))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_tool_has_a_name_a_description_and_an_object_schema() {
        for t in tools() {
            assert!(!t.name.is_empty());
            assert!(t.description.len() > 20, "{} describes itself", t.name);
            assert_eq!(t.input_schema["type"], "object", "{} takes an object", t.name);
        }
    }

    #[test]
    fn tool_names_are_snake_case_and_unique() {
        let mut seen = Vec::new();
        for t in tools() {
            assert!(t.name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'), "{}", t.name);
            assert!(!seen.contains(&t.name), "{} twice", t.name);
            seen.push(t.name);
        }
    }

    #[test]
    fn the_commands_every_tool_runs_are_real_commands() {
        let all: Vec<&str> = crate::all_commands().iter().map(|c| c.id).collect();
        for t in table() {
            if let Some(id) = t.command {
                assert!(all.contains(&id), "{} runs {id}, which is not a command", t.name);
            }
        }
    }

    #[test]
    fn the_image_tools_are_the_media_ones() {
        assert!(is_media_tool("generate_image"));
        assert!(is_media_tool("stock_photo"));
        assert!(is_media_tool("vectorize_image"));
        assert!(is_media_tool("remove_background"));
        assert!(!is_media_tool("create_node"));
        assert!(!is_media_tool("mcp__notes__search"));
    }

    #[test]
    fn a_tool_that_runs_a_command_answers_with_the_command_s_output() {
        let mut s = crate::Session::empty();
        let _ = s.execute("file.new", &json!({}));
        let create = table().iter().find(|t| t.name == "create_node").expect("create_node is a tool");
        let out = create.run(&mut s, &json!({"type": "rectangle", "width": 10, "height": 10})).expect("the tool runs");
        assert!(out.get("id").is_some(), "the answer names the layer");
    }

    #[test]
    fn a_tool_s_failure_is_the_answer_not_a_panic() {
        let mut s = crate::Session::empty();
        let get = table().iter().find(|t| t.name == "get_node").expect("get_node is a tool");
        let err = get.run(&mut s, &json!({"id": 999_999})).expect_err("a missing layer is an error");
        assert!(!err.is_empty());
    }

    #[test]
    fn the_tools_run_as_the_agent_not_as_the_person() {
        let mut s = crate::Session::empty();
        let _ = s.execute("file.new", &json!({}));
        let create = table().iter().find(|t| t.name == "create_node").expect("create_node is a tool");
        let _ = create.run(&mut s, &json!({"type": "rectangle", "width": 10, "height": 10}));
        // The change is an undo step: it can be taken back.
        let undoable = s.docs.last().map(|d| !d.history.undo.is_empty()).unwrap_or(false);
        assert!(undoable, "the change is undoable");
    }

    #[test]
    fn run_command_runs_whatever_command_it_names() {
        let mut s = crate::Session::empty();
        let _ = s.execute("file.new", &json!({}));
        let run = table().iter().find(|t| t.name == "run_command").expect("run_command is a tool");
        let out = run.run(&mut s, &json!({"command": "node.create", "params": {"type": "ellipse", "width": 5, "height": 5}})).expect("the tool runs");
        assert!(out.get("id").is_some());
    }

    #[test]
    fn run_command_without_a_command_is_an_error() {
        let mut s = crate::Session::empty();
        let run = table().iter().find(|t| t.name == "run_command").expect("run_command is a tool");
        assert!(run.run(&mut s, &json!({})).is_err());
    }

    #[test]
    fn the_pixel_tools_say_they_need_the_interface() {
        let mut s = crate::Session::empty();
        for name in ["screenshot", "review_design"] {
            let shot = table().iter().find(|t| t.name == name).expect("the tool is listed");
            let err = shot.run(&mut s, &json!({})).expect_err("no pixels from the engine");
            assert!(err.contains("interface"), "{name}: {err}");
        }
    }

    #[test]
    fn an_invented_tool_names_the_one_that_does_the_job() {
        // A model reaches for `read` when it wants a layer back; the answer has to say
        // `get_node`, or the next call is another guess.
        for (invented, right) in [
            ("read", "get_node"),
            ("read_file", "get_node"),
            ("create_frame", "create_node"),
            ("set_fill", "set_properties"),
            ("render", "screenshot"),
            ("multi", "batch"),
            ("something_else", "list_commands"),
        ] {
            let answer = instead_of(invented);
            assert!(answer.contains(right), "{invented}: {answer}");
        }
    }

    #[test]
    fn list_commands_lists_every_command() {
        let all = crate::cmd::commands_json();
        let listed = all.as_array().expect("a list").iter().filter_map(|c| c.get("id").and_then(Value::as_str)).count();
        assert_eq!(listed, crate::all_commands().len());
    }

    /// The audit is the answer to "the model cannot see that its headline is clipped". It has to
    /// be a tool the model can call, and it has to come back with numbers, not adjectives.
    #[test]
    fn the_layout_audit_is_a_tool_and_answers_with_the_numbers() {
        let mut s = crate::Session::empty();
        let _ = s.execute("file.new", &json!({}));
        let frame = s.execute("node.create", &json!({"type": "frame", "width": 300, "height": 300})).expect("a frame");
        let text = s
            .execute(
                "text.create",
                &json!({"parent": frame["id"].clone(), "characters": "One kiln. Four hands. No shortcuts.", "props": {"fontSize": 40}}),
            )
            .expect("text");
        let text_id = text["id"].as_i64().expect("an id");
        let _ = s.execute("node.set", &json!({"ids": [text_id], "props": {"textAutoResize": "none", "width": 300, "height": 40}}));

        let audit = table().iter().find(|t| t.name == "audit_layout").expect("the audit is a tool");
        let answer = audit.run(&mut s, &json!({})).expect("the audit runs");
        let findings = answer["findings"].as_array().expect("findings");
        let mine = findings.iter().find(|f| f["id"] == text_id).expect("the clipped text is named");
        assert!(mine["detail"].as_str().unwrap_or_default().contains("300"), "the box is quoted in pixels: {mine}");
        assert!(mine["fix"].as_str().is_some(), "every finding says how to fix it: {mine}");
    }

    #[test]
    fn inspect_reads_the_document_in_front() {
        let mut s = crate::Session::empty();
        let _ = s.execute("file.new", &json!({}));
        let read = table().iter().find(|t| t.name == "inspect_document").expect("inspect_document is a tool");
        assert!(read.run(&mut s, &json!({})).is_ok());
        assert!(read.run(&mut s, &json!({"outline": true})).is_ok());
    }

    #[test]
    fn icon_tools_search_and_add_to_canvas() {
        let mut s = crate::Session::empty();
        let _ = s.execute("file.new", &json!({}));
        let frame = s.execute("node.create", &json!({"type": "frame", "width": 400, "height": 400})).expect("a frame");
        let frame_id = frame["id"].as_i64().expect("frame id");

        // 1. Search tool
        let search = table().iter().find(|t| t.name == "search_icons").expect("search_icons tool");
        let results = search.run(&mut s, &json!({"query": "star"})).expect("search runs");
        let list = results.as_array().expect("array of icons");
        assert!(list.iter().any(|v| v.as_str() == Some("fa6-solid:star")));

        // 2. Add icon tool
        let add = table().iter().find(|t| t.name == "add_icon_to_canvas").expect("add_icon_to_canvas tool");
        let inserted = add
            .run(
                &mut s,
                &json!({
                    "icon_id": "fa6-solid:star",
                    "parent_frame_id": frame_id,
                    "layout_position": "top-right",
                    "color": "#bfbcfc"
                }),
            )
            .expect("insert runs");
        assert!(inserted.get("id").is_some());
        assert_eq!(inserted["icon_id"], "fa6-solid:star");
    }

    #[test]
    fn market_tools_search_and_add_to_canvas() {
        let mut s = crate::Session::empty();
        let _ = s.execute("file.new", &json!({}));
        let frame = s.execute("node.create", &json!({"type": "frame", "width": 800, "height": 600})).expect("a frame");
        let frame_id = frame["id"].as_i64().expect("frame id");

        // 1. Search market tool
        let search = table().iter().find(|t| t.name == "search_market_components").expect("search_market_components tool");
        let results = search.run(&mut s, &json!({"query": "kanban"})).expect("search runs");
        let list = results.as_array().expect("array of components");
        assert!(list.iter().any(|v| v["id"].as_str() == Some("@kibo/kanban-board")));

        // 2. Add market component tool
        let add = table().iter().find(|t| t.name == "add_market_component_to_canvas").expect("add_market_component_to_canvas tool");
        let inserted = add
            .run(
                &mut s,
                &json!({
                    "component_id": "@kibo/kanban-board",
                    "parent_frame_id": frame_id,
                    "position": {"x": 50, "y": 50},
                    "custom_text": {"Title": "Sprint 42 Kanban"}
                }),
            )
            .expect("insert runs");
        assert!(inserted.get("id").is_some());
        assert_eq!(inserted["component_id"], "@kibo/kanban-board");

        // 3. Verify Button component has white text, drop shadow effect, and child label
        let btn_inserted = add
            .run(
                &mut s,
                &json!({
                    "component_id": "@shadcn/button",
                    "position": {"x": 100, "y": 100},
                    "custom_text": {"Label": "Checkout Now"}
                }),
            )
            .expect("button insert runs");
        let btn_id_val = btn_inserted.get("id").expect("btn id");
        let btn_id = crate::params::id_of(btn_id_val).expect("node id");
        let doc = s.doc().expect("doc");
        let btn_node = doc.node(btn_id).expect("btn node");
        assert_eq!(btn_node.name, "Button / Primary");
        assert!(!btn_node.effects.is_empty(), "button must have drop shadow effects");
        assert!(!btn_node.children.is_empty(), "button must have child elements");
        let child_text = doc.node(btn_node.children[0]).expect("child text node");
        if let figcraft_doc::NodeKind::Text(td) = &child_text.kind {
            assert_eq!(td.characters, "Checkout Now");
        } else {
            panic!("child node should be text");
        }

        // 4. Verify Metric KPI Card has drop shadow, border stroke, and rich sub-components
        let metric_inserted = add
            .run(
                &mut s,
                &json!({
                    "component_id": "@shadcn/card-metric",
                    "position": {"x": 200, "y": 200},
                    "custom_text": {"Title": "Monthly MRR", "Value": "$84,200.00"}
                }),
            )
            .expect("metric insert runs");
        let metric_id = crate::params::id_of(metric_inserted.get("id").expect("metric id")).expect("node id");
        let metric_node = s.doc().expect("doc").node(metric_id).expect("metric node");
        assert_eq!(metric_node.name, "Card / Metric KPI");
        assert!(!metric_node.effects.is_empty(), "metric card must have drop shadow");
        assert!(!metric_node.strokes.is_empty(), "metric card must have border stroke");
        assert!(metric_node.children.len() >= 3, "metric card must have header, value, and trend children");

        // 5. Verify Neo-Brutalist button has hard black stroke and shadow
        let neo_inserted = add
            .run(
                &mut s,
                &json!({
                    "component_id": "@neobrutalism/neo-button",
                    "position": {"x": 300, "y": 300}
                }),
            )
            .expect("neo insert runs");
        let neo_id = crate::params::id_of(neo_inserted.get("id").expect("neo id")).expect("node id");
        let neo_node = s.doc().expect("doc").node(neo_id).expect("neo node");
        assert_eq!(neo_node.name, "Button / Neobrutalism");
        assert_eq!(neo_node.corners.radii[0], 6.0);
        assert!(!neo_node.effects.is_empty(), "neo button must have hard shadow");
        assert!(!neo_node.strokes.is_empty(), "neo button must have solid black stroke");
    }
}
