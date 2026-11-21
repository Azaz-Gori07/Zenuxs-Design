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
    let right = if has(&["icon", "icons", "symbol", "symbols"]) {
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
            description: "Search 200,000+ vector icons (Iconify, Lucide, Phosphor, Material). Returns a list of matching icon identifiers.",
            schema: obj(json!({"query": s("The icon name, keyword, or category (e.g. 'home', 'user', 'settings', 'arrow')")}), &["query"]),
            command: None,
        },
        ChatTool {
            name: "add_icon_to_canvas",
            description: "Insert a vector icon onto the canvas with layout context or exact coordinates.",
            schema: obj(
                json!({
                    "icon_id": s("The icon identifier (e.g. 'lucide:home', 'lucide:settings', 'lucide:star')"),
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

/// Search curated and online icon collections by keyword.
fn search_icons(arguments: &Value) -> Result<Value, String> {
    let query = arguments.get("query").and_then(Value::as_str).unwrap_or_default().trim().to_lowercase();
    let catalog = [
        "lucide:shapes",
        "lucide:star",
        "lucide:heart",
        "lucide:sparkles",
        "lucide:home",
        "lucide:file",
        "lucide:layers",
        "lucide:settings",
        "lucide:search",
        "lucide:user",
        "lucide:send",
        "lucide:paperclip",
        "lucide:check",
        "lucide:x",
        "lucide:plus",
        "lucide:minus",
        "lucide:timer",
        "lucide:pause",
        "lucide:globe",
        "lucide:lock",
        "lucide:layout-grid",
        "lucide:list",
        "lucide:arrow-down",
        "lucide:arrow-right",
        "lucide:arrow-left",
        "lucide:arrow-up",
        "lucide:pen",
        "lucide:pen-tool",
        "lucide:highlighter",
        "lucide:git-commit",
        "lucide:stamp",
        "lucide:bell",
        "lucide:image",
        "lucide:calendar",
        "lucide:mail",
        "lucide:trash",
        "lucide:edit",
        "lucide:eye",
        "lucide:shopping-cart",
        "lucide:credit-card",
        "lucide:folder",
        "ph:user",
        "ph:house",
        "ph:gear",
        "ph:magnifying-glass",
        "ph:heart",
        "material:home",
        "material:search",
        "material:favorite",
        "material:person",
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
        assert!(list.iter().any(|v| v.as_str() == Some("lucide:star")));

        // 2. Add icon tool
        let add = table().iter().find(|t| t.name == "add_icon_to_canvas").expect("add_icon_to_canvas tool");
        let inserted = add
            .run(
                &mut s,
                &json!({
                    "icon_id": "lucide:star",
                    "parent_frame_id": frame_id,
                    "layout_position": "top-right",
                    "color": "#bfbcfc"
                }),
            )
            .expect("insert runs");
        assert!(inserted.get("id").is_some());
        assert_eq!(inserted["icon_id"], "lucide:star");
    }
}
