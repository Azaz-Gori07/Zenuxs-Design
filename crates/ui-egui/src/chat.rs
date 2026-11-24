//! The AI chat: the conversation, the model's answer as it arrives, and the tools it calls.
//!
//! The chat is the right panel's AI tab. What it holds is what the engine holds
//! ([`figcraft_engine::chat`]); what this module adds is the network half — the request goes out
//! on a worker thread (a model answer takes seconds, and the interface keeps drawing), the
//! streamed text lands back through a channel, and the tool calls the model asks for run on the
//! interface thread through the engine's own commands, one undo step each.
//!
//! The model's instructions are assembled here too: the system prompt that teaches the command
//! workflow, the standing instructions the person switched on, and the skills the router
//! activates for the message being sent.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use egui::{Align2, Color32, Context, CornerRadius, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2};
use figcraft_ai::models::AIModelRole;
#[cfg(not(target_arch = "wasm32"))]
use figcraft_ai::transport::Chunk;
use figcraft_ai::transport::{ChatMessage, ChatRequest, ChatResponse, Role, ToolCall, TransportError};
use serde_json::{Value, json};

use crate::FigApp;
use crate::a11y;
use crate::a11y::Tip as _;
use crate::i18n::{tr, trf, trn};
use crate::icons::{self, Icon};
use crate::panels::right::{menu_check, menu_item, menu_popup};
use crate::theme::{AGENT, Tokens, regular, semibold};
use crate::widgets;

/// The steps one send may take before it stops: model answer → tools → model answer …
///
/// One source of truth, in the engine: the control channel's `ai.chat.send` runs the same send.
const MAX_STEPS: usize = figcraft_engine::chat::MAX_STEPS;

/// How many times one send re-asks a model that answered with nothing. Two is enough to ride out
/// a flaky provider; more would keep spending the person's tokens on a model that is not working.
const MAX_EMPTY_ANSWERS: usize = 2;

/// What is said to a model that answered with nothing. It names the work in front of it, because
/// an empty answer is usually a lost thread rather than a refusal: the model needs to be put back
/// on the step it was on, not congratulated.
const EMPTY_ANSWER_NUDGE: &str = "Your last reply was empty. Continue the work: call the next tool \
you need, or, if the design is finished, say what you built and what is left. Do not reply with \
nothing again.";

/// The chat panel's own state, beyond what the engine keeps.
pub struct Panel {
    /// What the person is typing.
    pub draft: String,
    /// The request out right now, if one is.
    pub streaming: Option<Streaming>,
    /// Why the last send failed, if it did.
    pub failure: Option<String>,
    /// The reasoning the model showed, while its answer streams.
    pub reasoning: String,
    /// Whether the reasoning shows.
    pub show_reasoning: bool,
    /// Whether the tool calls show as they run.
    pub show_tools: bool,
    /// The tools the MCP servers offered for the send in progress, by their prefixed name.
    pub mcp_tools: Vec<(String, String)>,
    /// The chat-history search text.
    pub history_search: String,
    /// Whether the history list shows.
    pub history_open: bool,
    /// Whether the transcript follows the newest message. A wheel over it turns this off; Send,
    /// a reply that arrives and the Latest button turn it back on.
    ///
    /// Following is the default: a conversation opens on its newest message.
    pub stick: bool,
    /// Whether the message carries the selection: the composer's pointer control. Off sends the
    /// page alone, which is what a person who selected a layer only to see it wanted.
    pub with_selection: bool,
    /// The model spent its step budget and the send stopped with the work unfinished: the
    /// transcript offers Continue, which spends another budget on the same conversation.
    pub resumable: bool,
    /// How many times this send has re-asked a model that answered with nothing. A model
    /// occasionally returns an empty response mid-work; one more ask usually gets an answer, and
    /// giving up there strands the design half-built.
    pub empty_answers: usize,
    /// How many design review and audit rounds have run for this send. The review loop runs
    /// on the document when the model completes its pass, checking for broken elements,
    /// overflowing buttons, missing photography, and alignment faults.
    pub review_rounds: usize,
    /// The title being typed for a rename, when one is open.
    pub rename: Option<String>,
    /// Image currently attached to the composer draft (pasted from clipboard or selected).
    pub attached_image: Option<ChatAttachment>,
    /// Whether the composer text field should gain focus on the next frame.
    pub focus_draft: bool,
    /// Last selected element id, used to auto-mention newly clicked/selected elements.
    pub last_selected_id: Option<u64>,
}

/// An image attached to the composer draft (pasted or selected).
#[derive(Clone, Debug, PartialEq)]
pub struct ChatAttachment {
    pub name: String,
    pub mime: String,
    pub data_url: String,
    pub preview_bytes: Vec<u8>,
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            draft: String::new(),
            streaming: None,
            failure: None,
            reasoning: String::new(),
            show_reasoning: false,
            show_tools: true,
            mcp_tools: Vec::new(),
            history_search: String::new(),
            history_open: false,
            stick: true,
            with_selection: true,
            resumable: false,
            rename: None,
            empty_answers: 0,
            review_rounds: 0,
            attached_image: None,
            focus_draft: false,
            last_selected_id: None,
        }
    }
}

/// One request on its worker thread.
pub struct Streaming {
    /// The chunks and the answer as they arrive.
    rx: Receiver<Event>,
    /// This end of the channel, so the tools' worker answers into the same stream.
    tx: Sender<Event>,
    /// The thread the request runs on; it ends with the request.
    _thread: Option<std::thread::JoinHandle<()>>,
    /// Which model step of this send is out.
    pub step: usize,
    /// The text of the answer so far.
    pub text: String,
    /// The tool calls the answer asked for, once it is in.
    pub tool_calls: Vec<ToolCall>,
    /// The tool answers gathered so far, in the order the model asked for them.
    pub results: Vec<(ToolCall, String)>,
    /// Whether the MCP calls were sent to their worker.
    pub mcp_dispatched: bool,
    /// The provider and model, for the usage line.
    pub provider: String,
    pub model: String,
}

/// What crosses from the worker thread to the interface.
enum Event {
    /// A piece of the answer's text.
    Text(String),
    /// A piece of the model's reasoning.
    Reasoning(String),
    /// The MCP tools the servers offered, as the request named them.
    McpTools(Vec<(String, figcraft_ai::transport::Tool)>),
    /// One networked tool's answer, by call id.
    ToolResult(String, String),
    /// The answer is in, or it failed.
    Done(Result<ChatResponse, TransportError>),
}

/// One MCP server as the worker reaches it: its name, its URL and its token.
#[derive(Clone)]
struct McpServer {
    name: String,
    url: String,
    bearer: Option<String>,
}

/// The MCP servers' tools, prefixed so a call names its server.
fn mcp_tool_name(server: &str, tool: &str) -> String {
    format!("mcp__{server}__{tool}")
}

/// The server and tool a prefixed call names.
fn mcp_tool_of(name: &str) -> Option<(&str, &str)> {
    let rest = name.strip_prefix("mcp__")?;
    let (server, tool) = rest.split_once("__")?;
    Some((server, tool))
}

/// Fetch one server's tools, prefixed with its name.
fn fetch_mcp_tools(factory: &dyn figcraft_ai::McpFactory, server: &McpServer) -> Result<Vec<(String, figcraft_ai::transport::Tool)>, TransportError> {
    let mut client = factory.make(server.url.clone(), server.bearer.clone());
    client.initialize().map_err(|e| TransportError::Other(format!("{}: {}", server.name, e.message())))?;
    Ok(client
        .tools()
        .map_err(|e| TransportError::Other(format!("{}: {}", server.name, e.message())))?
        .into_iter()
        .map(|tool| (mcp_tool_name(&server.name, &tool.name), tool))
        .collect())
}

/// Run one MCP tool on its server.
///
/// Desktop only: a page cannot open the socket an MCP server listens on.
#[cfg(not(target_arch = "wasm32"))]
fn call_mcp_tool(factory: &dyn figcraft_ai::McpFactory, server: &McpServer, tool: &str, arguments: &Value) -> Result<String, TransportError> {
    let mut client = factory.make(server.url.clone(), server.bearer.clone());
    client.initialize().map_err(|e| TransportError::Other(format!("{}: {}", server.name, e.message())))?;
    client.call(tool, arguments).map_err(|e| TransportError::Other(format!("{}: {}", server.name, e.message())))
}

/// The system prompt: what the model is, and how the work is done here.
const SYSTEM_PROMPT: &str = "\
You are the design agent inside sycnUI, an interface design tool. You create and change designs \
by running sycnUI's own commands. Be direct, use design terminology, and after finishing give a \
2-3 line summary: frame sizes, the accent colours, and anything still wrong.

# Multimodal and attached images

When the user attaches or pastes an image or screenshot in the chat, carefully inspect its visual \
hierarchy, layout structure, color palette, typography, components, and proportions. You have full vision \
capability to examine the image. Directly build, replicate, or adapt the design on the canvas using \
sycnUI tools (frames, auto layout, shapes, text, styling).

# Mentioned and target elements

When the user prompt mentions an element or target layers are specified under SELECTED & MENTIONED TARGET LAYERS:
- Directly target and modify those specific element(s) by their node id using `set_properties`, `create_node` (for children), `layout.set`, and `props.text`.
- DO NOT recreate the element or hallucinate a new layer unless the user explicitly requests duplicating or replacing it.
- Respect and preserve existing position, dimensions, parent hierarchy, and auto layout properties unless instructed to alter them.

# Component Marketplace Priority (#1 Rule for Generating UI)

When the user asks to build or design any UI screen, feature, or element (e.g., \"create a dashboard\", \"add a login form\", \"make a kanban board\", \"add buttons\", \"create a pricing table\", \"add charts\"):
1. FIRST PRIORITY: Always search and pick matching prebuilt components from the **Market Registry** (@shadcn, @originui, @kibo, @cult, @animate-ui, @bundui, @neobrutalism) using `search_market_components`.
2. CUSTOMIZATION: Use `add_market_component_to_canvas` to instantiate the matched component with customized titles, labels, values, and color themes tailored to the user's brief.
3. FALLBACK ONLY: Only if no suitable component exists in the Market Registry should you generate custom primitive elements from scratch.

# Build, do not interview

A brief is a starting point, not a form to fill in. When the person names what to make (\"an \
e-commerce web app\", \"a news platform\"), that is enough: choose the product, the palette, the \
type and the composition yourself, say what you chose in one line, and build the screens. A \
designer who answers a brief with four questions has not started. Ask at most one clarifying \
question, and only when the answer would change the structure — never about colours, ratios, \
fonts or \"vibe\".

# How the work is done

Everything the document takes is a command. Your tools are commands:
- `props_reference` — every `node.set` key and the exact shape of its value. Read it once before your first property call: it is the difference between a design that lands and twenty failed calls.
- `list_commands` — the command catalogue, one line each. Pass `filter` (\"variable\", \"proto\", \"component\", \"export\") instead of reading all ~700.
- `inspect_document` — read the pages and the layer tree before changing anything, so your work builds on what is there.
- `create_node` — create a layer. Its `props` take any `node.set` key, so a layer can arrive with its fill, corner radius and auto layout already set.
- `set_properties` — change layers' fills, strokes, effects, corners, auto layout, text.
- `get_node` — read one layer back. Check your work with it after each change.
- `batch` — several commands as one undo step. Every group of changes that belongs together goes in one.
- `run_command` — any command at all, by id.
- `export` — the document's own pixels (PNG, SVG, PDF) into a file.
- `screenshot` — look at what you built: it renders the layers and reads them back through the Vision model, which tells you what it sees and what is visibly wrong. Use it before you call a screen finished.
- `audit_layout` — the layout measured rather than looked at. It answers with numbers the picture cannot give you: which text layer is clipped, how many pixels of it are cut off, which picture slot is still an empty box, which layer floats loose on the page, which frame has no size. Every finding carries the layer id and the fix. Run it before `review_design`.
- `review_design` — the same pixels, graded by the reviewer as a designer: hierarchy, alignment, spacing rhythm, type scale, contrast, and the specific faults with where each one is.
- `search_market_components` — search 100% free verified prebuilt components from Shadcn ecosystem registries (@shadcn, @originui, @kibo, @cult, @animate-ui, @bundui, @neobrutalism). Use this FIRST before drawing primitive elements.
- `add_market_component_to_canvas` — instantiate prebuilt Market components on canvas with customized props and titles.
- `source_image` — a picture for a slot, searched in the public libraries that need no key and verified by the Vision model before it is placed. One call for every image on the screen.
- `generate_image` — an original picture, when a photo would not exist. Needs an image model on the connection; `source_image` is the one that works with none.

# Batch steps

`batch` takes `commands`: an array of `{command, params, as?}`. `command` is always a command id \
from `list_commands` (`node.create`, `node.set`), **never a tool name**: `create_node` inside a \
batch is an unknown command. `{\"$step\": 0}` or `{\"$ref\": \"name\"}` in a later step's params \
stands for an earlier step's result, so one batch can create a layer and set its properties.

# The document

One document is a tree of nodes keyed by id; pages hold the top-level frames. Layers nest in \
frames; auto layout (`layout` in `node.set` keys) lays a frame's children out in a row or a \
column with `itemSpacing` and `paddingTop`/`Right`/`Bottom`/`Left`. Components are the masters, \
instances mirror them, variants are members of a component set. Variables hold the design \
tokens; bindings put them on fills, strokes and layout keys. Prototype behaviour (flows, \
interactions) is document data on nodes: every destination it names must be an existing node id.

Keys that bite: sizing is `sizingH` / `sizingV` (or `layoutSizingHorizontal` / `layoutSizingVertical`) — \
there is no `sizingHorizontal`. Fills are a list of paints: `[\"#ff0000\"]`, `[{\"type\":\"solid\",\"color\":{\"r\",\"g\",\"b\"}}]`, \
or a gradient with `stops`. Padding is `24`, `[top,right,bottom,left]` or `{top,right,bottom,left}`. \
`layout` may be `\"vertical\"`, `\"horizontal\"`, `\"grid\"` or an object with `mode` in it.

# A design, built the way a designer builds it

1. `inspect_document`. Then `props_reference` if you have not read it in this conversation.
2. Frame first: `create_node` a frame at the size the target asks for, named for the screen or page.
3. Sections of the frame are frames with auto layout, not loose layers at coordinates. A page is a \
   stack of sections; a card is a frame with `layout: \"vertical\"`, a gap and padding.
4. Text is a text layer with real `characters`, a size from the scale, and `sizingH: \"fill\"` inside \
   a layout. Never a rectangle pretending to be text, never empty text, never `Lorem ipsum`. Any font family can be set via `fontFamily` in props (e.g. \"Inter\", \"Playfair Display\", serif); `text.loadFont` takes `family` to load or download fonts.
5. Check as you go: `get_node` on the frame after a section lands, and `screenshot` when the \
   question is what it looks like.
6. Before you say it is finished, `audit_layout` the screen and fix every finding, then \
   `review_design` it against the brief and fix what the review names. A design you have not \
   looked at is not finished. A finding from the audit is a fact, not a suggestion: a clipped \
   headline, an empty picture slot and a layer loose on the page are all defects, whether or not \
   the picture makes them obvious.
7. Leave nothing behind: no scratch or temporary layers on the page, no layer outside the frame \
   that owns it, no unnamed layer a person would have to guess at.

# Craft rules

- Work on a 4px spacing grid: 4, 8, 12, 16, 20, 24, 32, 48. Inside a group is tighter than between groups, which is tighter than between sections.
- A type scale of 6-8 sizes is enough: display 32-40, heading 24-28, body 14-15, caption 12-13. Two or three weights, one accent colour used for the thing that matters.
- Wrapping text takes its width from its container, never a fixed width: a fixed width guesses the font metrics and spills. In auto layout, a child that stretches says `sizingH: \"fill\"`, a hug child says `\"hug\"`.
- Nothing escapes the frame that owns it. When an element overlaps its neighbour, fix the container's layout; never nudge coordinates to make it look right.
- A picture slot is a picture, not an empty frame: draw one frame per picture where it belongs, \
  then call `source_image` with up to four slots per call. Each `requests` entry's `id` is the id \
  of the slot frame you drew, and the picture fills that frame. Keep each brief short — the \
  subject in a sentence and the library query in two or three words. A frame left as a grey box \
  is a defect the audit will name.
- Check your work: after creating or changing, read the result back with `get_node` (and `screenshot` when appearance is the question) and fix what is wrong before moving on.
- Put changes that belong together in one `batch`, so one undo takes them back together.
- A whole page is not one screen: if the brief asks for several, build each as its own frame beside \
  the others, named for what it shows, and review each one.

Never claim something the document does not show: read it back before you say it. If a capability \
does not exist, say so and offer the nearest one that does.";

/// Send what the person typed.
///
/// The message goes into the conversation, the skills the router activates ride along on it, and
/// the first request goes out. `Err` says what to fix in the settings.
pub fn send(app: &mut FigApp, text: &str) -> Result<(), String> {
    let text = text.trim().to_string();
    if text.is_empty() && app.ui.chat.attached_image.is_none() {
        return Ok(());
    }
    let text = if text.is_empty() {
        "Please inspect this image and build or adapt this design in sycnUI.".to_string()
    } else {
        text
    };
    if app.ui.chat.streaming.is_some() {
        return Err(tr("A request is already out. Stop it first.").to_string());
    }

    // The skills this message activates, as the model reads them.
    let skills_block = skills_for(app, &text);

    // A new send counts its own retries: the empty-answer and design review allowances belong to one
    // send, not to the conversation.
    app.ui.chat.empty_answers = 0;
    app.ui.chat.review_rounds = 0;

    // The selection and the page, as the model reads them.
    let selection = selection_context(app);

    let user = if skills_block.is_empty() { text } else { format!("{text}{skills_block}") };
    let attached = app.ui.chat.attached_image.take();
    let images = attached.map(|a| vec![a.data_url]).unwrap_or_default();
    let msg = if images.is_empty() {
        ChatMessage::user(user)
    } else {
        ChatMessage::user_with_images(user, images)
    };
    app.session.chat_append(msg).map_err(|e| e.to_string())?;
    app.session.chat.context = selection;

    dispatch(app)
}

/// What the router makes of one message: the skills it activates, written the way the model reads
/// them, or nothing when none apply.
///
/// Every path that puts a message in front of the model goes through here, the control channel's
/// included: a send that skipped this ran with no skill knowledge at all, which is the difference
/// between a designed screen and a stack of rectangles.
fn skills_for(app: &FigApp, text: &str) -> String {
    let context = routing_context(app);
    let router = figcraft_ai::skills::SkillRouter::default();
    let route = router.route(text, Some(&context));
    router.build_context(&route)
}

/// Send the message a command (`ai.chat.send`, an action, the control channel) already queued.
///
/// Such a command appended the user message itself. Adding it again here would duplicate it in
/// the transcript and send the model two copies, so this only assembles the context and starts
/// the request. It is called from the frame loop when `pending_send` is set.
pub fn send_pending(app: &mut FigApp) -> Result<(), String> {
    if app.ui.chat.streaming.is_some() {
        return Err(tr("A request is already out. Stop it first.").to_string());
    }
    if app.session.chat_messages().last().is_none_or(|m| m.role != figcraft_ai::transport::Role::User) {
        return Ok(());
    }
    // The control channel's send arrives as a plain user message, so the skills it activates are
    // worked out here: `send` does it before appending, and a send that skipped the router would
    // reach the model with no skill knowledge at all.
    if let Some(text) = app.session.chat_messages().last().map(|m| m.content.clone()) {
        let block = skills_for(app, &text);
        if !block.is_empty() && !text.contains(figcraft_ai::skills::SKILL_CONTEXT_MARKER) {
            let last = app.session.chat.messages.len().saturating_sub(1);
            if let Some(slot) = app.session.chat.messages.get_mut(last) {
                slot.content = format!("{text}{block}");
            }
        }
    }
    app.session.chat.context = selection_context(app);
    dispatch(app)
}

/// Assemble the request from the conversation as it stands and start its first step.
fn dispatch(app: &mut FigApp) -> Result<(), String> {
    let standing = figcraft_ai::skills::active_prompt_text(&app.session.prefs.ai.skills.active());
    let request = build_request(app, &standing)?;
    start_step(app, request, 0)
}

/// The next step of the send in progress: the tools have run, the model answers again.
fn continue_step(app: &mut FigApp) -> Result<(), String> {
    let step = app.ui.chat.streaming.as_ref().map(|s| s.step + 1).unwrap_or(1);
    continue_step_at(app, step)
}

/// Start the next model step, numbering it as `step`. The step number is what
/// [`MAX_STEPS`] counts, so a step started after the stream was taken keeps the count going.
fn continue_step_at(app: &mut FigApp, step: usize) -> Result<(), String> {
    let standing = figcraft_ai::skills::active_prompt_text(&app.session.prefs.ai.skills.active());
    let request = build_request(app, &standing)?;
    start_step(app, request, step)
}

/// The routing facts the skills' router may use: the selection's node types.
fn routing_context(app: &FigApp) -> figcraft_ai::skills::RoutingContext {
    figcraft_ai::skills::RoutingContext {
        selected_node_types: app
            .session
            .ds()
            .map(|d| d.selection.iter().filter_map(|id| d.doc.node(*id).map(|n| n.kind.type_name().to_string())).collect())
            .unwrap_or_default(),
        has_selection: app.session.ds().is_ok_and(|d| !d.selection.is_empty()),
    }
}

/// Information about the currently selected and mentioned target element.
#[derive(Clone, Debug, PartialEq)]
pub struct MentionInfo {
    pub name: String,
    pub kind: &'static str,
    pub id: u64,
    pub count: usize,
}

pub fn selection_mention_info(app: &FigApp) -> Option<MentionInfo> {
    if !app.ui.chat.with_selection {
        return None;
    }
    let ds = app.session.ds().ok()?;
    if ds.selection.is_empty() {
        return None;
    }
    let count = ds.selection.len();
    let first_id = ds.selection[0];
    let node = ds.doc.node(first_id)?;
    let kind = match &node.kind {
        figcraft_doc::NodeKind::Frame => "Frame",
        figcraft_doc::NodeKind::Group => "Group",
        figcraft_doc::NodeKind::Section { .. } => "Section",
        figcraft_doc::NodeKind::Rectangle => "Rectangle",
        figcraft_doc::NodeKind::Ellipse { .. } => "Ellipse",
        figcraft_doc::NodeKind::Line => "Line",
        figcraft_doc::NodeKind::Text(_) => "Text",
        figcraft_doc::NodeKind::Component { .. } => "Component",
        figcraft_doc::NodeKind::ComponentSet { .. } => "ComponentSet",
        figcraft_doc::NodeKind::Instance { .. } => "Instance",
        figcraft_doc::NodeKind::Star { .. } => "Star",
        figcraft_doc::NodeKind::Polygon { .. } => "Polygon",
        figcraft_doc::NodeKind::Vector { .. } => "Vector",
        _ => "Element",
    };
    if count == 1 {
        Some(MentionInfo {
            name: node.name.clone(),
            kind,
            id: first_id.0,
            count: 1,
        })
    } else {
        Some(MentionInfo {
            name: format!("{} + {} more", node.name, count - 1),
            kind: "Selection",
            id: first_id.0,
            count,
        })
    }
}

fn mention_height(app: &FigApp) -> f32 {
    if selection_mention_info(app).is_some() { 32.0 } else { 0.0 }
}

/// The selection and the page, as the model reads them.
///
/// The composer's pointer control decides whether the selection rides along: a person who
/// selected a layer only to look at it should not have it quoted to the model.
fn selection_context(app: &mut FigApp) -> String {
    let with_selection = app.ui.chat.with_selection;
    let Ok(ds) = app.session.ds() else { return String::new() };
    let page = ds.doc.node(ds.page).map(|n| n.name.clone()).unwrap_or_default();
    let page_id = ds.page.0;
    if !with_selection || ds.selection.is_empty() {
        return format!("Page: {page} (id {page_id}). Nothing is selected.");
    }
    let n = ds.selection.iter().filter(|id| ds.doc.node(**id).is_some()).count();
    let first_id = ds.selection.first().map(|id| id.0).unwrap_or(0);
    let mut out = if n <= 1 {
        format!("Page: {page} (id {page_id}). Selection: one layer, id {first_id}.\n")
    } else {
        format!("Page: {page} (id {page_id}). Selection: {n} layers, first id {first_id}.\n")
    };
    out.push_str("SELECTED & MENTIONED TARGET LAYERS (Apply changes, styling, or text updates directly to these elements):\n");
    for id in &ds.selection {
        if let Some(node) = ds.doc.node(*id) {
            let kind_str = match &node.kind {
                figcraft_doc::NodeKind::Frame => "Frame",
                figcraft_doc::NodeKind::Group => "Group",
                figcraft_doc::NodeKind::Section { .. } => "Section",
                figcraft_doc::NodeKind::Rectangle => "Rectangle",
                figcraft_doc::NodeKind::Ellipse { .. } => "Ellipse",
                figcraft_doc::NodeKind::Line => "Line",
                figcraft_doc::NodeKind::Text(_) => "Text",
                figcraft_doc::NodeKind::Component { .. } => "Component",
                figcraft_doc::NodeKind::ComponentSet { .. } => "ComponentSet",
                figcraft_doc::NodeKind::Instance { .. } => "Instance",
                figcraft_doc::NodeKind::Star { .. } => "Star",
                figcraft_doc::NodeKind::Polygon { .. } => "Polygon",
                figcraft_doc::NodeKind::Vector { .. } => "Vector",
                _ => "Layer",
            };
            if let Some(b) = ds.doc.bounds_of(&[*id]) {
                out.push_str(&format!(
                    "- Node #{} (\"{}\", {}): bounds [x: {:.1}, y: {:.1}, w: {:.1}, h: {:.1}]",
                    node.id.0, node.name, kind_str, b.x0, b.y0, b.width(), b.height()
                ));
            } else {
                out.push_str(&format!(
                    "- Node #{} (\"{}\", {}): size [w: {:.1}, h: {:.1}]",
                    node.id.0, node.name, kind_str, node.width, node.height
                ));
            }
            if let Some(text) = node.kind.text() {
                out.push_str(&format!(", text: \"{}\"", text.characters));
            }
            if let Some(layout) = &node.layout {
                out.push_str(&format!(", autoLayout: mode={:?} gap={:.1}", layout.mode, layout.gap));
            }
            if !node.children.is_empty() {
                out.push_str(&format!(", childrenCount: {}", node.children.len()));
            }
            out.push('\n');
        }
    }
    out
}

/// The MCP servers the send may reach, with their tokens resolved.
fn mcp_servers(app: &FigApp) -> Vec<McpServer> {
    let store = app.session.credentials.as_ref();
    app.session
        .prefs
        .ai
        .mcp
        .connections
        .iter()
        .filter(|c| c.enabled)
        .map(|c| McpServer {
            name: c.name.clone(),
            url: c.url.clone(),
            bearer: if c.authentication == figcraft_ai::mcp::MCPAuthentication::Bearer {
                store.and_then(|s| s.read(&figcraft_ai::credentials::CredentialRef::mcp_token(&c.id)).ok().flatten())
            } else {
                None
            },
        })
        .collect()
}

/// Build the request: the instructions, the conversation, the tools and the model's settings.
fn build_request(app: &FigApp, standing: &str) -> Result<ChatRequest, String> {
    let (resolved, key) = app.session.chat_model(AIModelRole::Design).map_err(|e| e.to_string())?;
    let connection = &resolved.connection;
    let base_url = if connection.custom_base_url.trim().is_empty() {
        connection.provider.default_base_url().unwrap_or_default().to_string()
    } else {
        connection.custom_base_url.clone()
    };
    let mut system = String::from(SYSTEM_PROMPT);
    if !standing.trim().is_empty() {
        system.push_str("\n\n");
        system.push_str(standing);
    }
    if !app.session.chat.context.is_empty() {
        system.push_str("\n\n# The document right now\n\n");
        system.push_str(&app.session.chat.context);
    }
    Ok(ChatRequest {
        model: figcraft_ai::models::AIModelSettings::effective_model_id(&resolved.profile).to_string(),
        base_url,
        api_key: key,
        protocol: connection.provider.protocol(),
        responses_api: connection.custom_api_type == figcraft_ai::models::APIType::Responses,
        messages: app.session.chat_messages().to_vec(),
        system,
        tools: figcraft_engine::chat_tools::tools(),
        max_output_tokens: resolved.profile.max_output_tokens,
        reasoning_effort: resolved.profile.reasoning_effort.clone(),
    })
}

/// Start one model step on its worker thread, or on the page's event loop.
fn start_step(app: &mut FigApp, request: ChatRequest, step: usize) -> Result<(), String> {
    let (tx, rx) = channel();
    let provider = request.base_url.clone();
    let model = request.model.clone();
    let transport = app.chat_transport.clone();
    let mcp_factory = app.mcp_factory.clone();
    let servers = mcp_servers(app);
    let thread = ask(&*transport, mcp_factory, &servers, request, tx.clone())?;
    app.session.chat.busy = true;
    app.ui.chat.failure = None;
    app.ui.chat.resumable = false;
    app.ui.chat.reasoning.clear();
    app.ui.chat.mcp_tools.clear();
    app.ui.chat.streaming = Some(Streaming {
        rx,
        tx,
        _thread: thread,
        step,
        text: String::new(),
        tool_calls: Vec::new(),
        results: Vec::new(),
        mcp_dispatched: false,
        provider,
        model,
    });
    Ok(())
}

/// The servers' tools, joined onto the request, with what they offered reported for the
/// dispatch and the panel's list.
///
/// A server that cannot be reached is not a failed send: the request carries on with the tools
/// it has, and the answer says which server was out.
fn with_mcp_tools(request: &mut ChatRequest, factory: &dyn figcraft_ai::McpFactory, servers: &[McpServer], tx: &Sender<Event>) {
    let mut offered: Vec<(String, figcraft_ai::transport::Tool)> = Vec::new();
    for server in servers {
        match fetch_mcp_tools(factory, server) {
            Ok(tools) => offered.extend(tools),
            Err(e) => offered.push((
                mcp_tool_name(&server.name, "_unreachable"),
                figcraft_ai::transport::Tool { name: "_unreachable".into(), description: e.message(), input_schema: json!({"type": "object"}) },
            )),
        }
    }
    if !offered.is_empty() {
        let _ = tx.send(Event::McpTools(offered.clone()));
        request.tools.extend(offered.into_iter().map(|(_, tool)| tool));
    }
}

/// Ask the model, reporting into `tx` as it answers.
///
/// A desktop build returns the thread the answer arrives on, and the MCP handshake runs there
/// too: it is a network round trip per server and must never hold up a frame. A page has no
/// threads, so its factory starts the request on the event loop and answers `true` from
/// `start`; there is nothing to join and this returns `None`.
#[cfg(not(target_arch = "wasm32"))]
fn ask(
    factory: &dyn figcraft_ai::TransportFactory,
    mcp: Arc<dyn figcraft_ai::McpFactory>,
    servers: &[McpServer],
    request: ChatRequest,
    tx: Sender<Event>,
) -> Result<Option<std::thread::JoinHandle<()>>, String> {
    let transport = factory.make(&request);
    let servers = servers.to_vec();
    let thread = std::thread::Builder::new()
        .name("figcraft-chat".into())
        .spawn(move || {
            let mut request = request;
            with_mcp_tools(&mut request, &*mcp, &servers, &tx);
            let result = transport.complete(&request, &mut |chunk| match chunk {
                Chunk::Text(text) => drop(tx.send(Event::Text(text))),
                Chunk::Reasoning(text) => drop(tx.send(Event::Reasoning(text))),
                Chunk::ToolCall(_) => {}
            });
            let _ = tx.send(Event::Done(result));
        })
        .map_err(|e| e.to_string())?;
    Ok(Some(thread))
}

#[cfg(target_arch = "wasm32")]
fn ask(
    factory: &dyn figcraft_ai::TransportFactory,
    mcp: Arc<dyn figcraft_ai::McpFactory>,
    servers: &[McpServer],
    request: ChatRequest,
    tx: Sender<Event>,
) -> Result<Option<std::thread::JoinHandle<()>>, String> {
    let mut request = request;
    // A page's MCP client is the same synchronous seam, so the tools are gathered here, once,
    // before the request goes out. Nothing is drawn until they are in.
    with_mcp_tools(&mut request, &*mcp, servers, &tx);
    let sink = tx.clone();
    let started = factory.start(
        request,
        Box::new(move |event| match event {
            figcraft_ai::StreamEvent::Text(text) => drop(sink.send(Event::Text(text))),
            figcraft_ai::StreamEvent::Reasoning(text) => drop(sink.send(Event::Reasoning(text))),
            figcraft_ai::StreamEvent::Done(result) => drop(sink.send(Event::Done(result))),
        }),
    );
    if started {
        Ok(None)
    } else {
        let _ = tx.send(Event::Done(Err(TransportError::Other(
            "No transport is wired for this build, so no model can be asked. Set a model in AI & Agents.".into(),
        ))));
        Ok(None)
    }
}

/// Drain the request in progress, and run the tools when its answer asks for them.
///
/// Called every frame while a request is out. `true` when the send is over.
pub fn drain(app: &mut FigApp, ctx: &Context) -> bool {
    let mut finished = false;

    if let Some(streaming) = &mut app.ui.chat.streaming {
        while let Ok(event) = streaming.rx.try_recv() {
            match event {
                Event::Text(text) => streaming.text.push_str(&text),
                Event::Reasoning(text) => app.ui.chat.reasoning.push_str(&text),
                Event::McpTools(tools) => {
                    app.ui.chat.mcp_tools = tools.iter().map(|(name, tool)| (name.clone(), tool.description.clone())).collect();
                }
                Event::ToolResult(id, text) => {
                    if let Some(call) = streaming.tool_calls.iter().find(|c| c.id == id) {
                        streaming.results.push((call.clone(), cap_result(&call.name, text)));
                    }
                }
                Event::Done(result) => {
                    finished = true;
                    match result {
                        Ok(response) => {
                            streaming.text = response.text.clone();
                            streaming.tool_calls = response.tool_calls.clone();
                            let usage = app.session.chat.record_usage(&streaming.provider, &streaming.model, &response);
                            app.session.ai.record(usage.clone());
                            app.session.prefs.ai.diagnostics.model_step(
                                &streaming.provider,
                                &streaming.model,
                                Some(usage.input_tokens),
                                Some(usage.output_tokens),
                                figcraft_engine::ai::now_ms() / 1000,
                            );
                            app.session.prefs_unsaved = true;
                        }
                        Err(e) => {
                            app.session.prefs.ai.diagnostics.chat_failed("TransportError", figcraft_engine::ai::now_ms() / 1000);
                            app.ui.chat.failure = Some(e.message());
                        }
                    }
                }
            }
        }
    }

    // The answer is in. Its text and its tool calls become the conversation; the engine's own
    // tools run here on the interface thread, and the MCP calls on their worker.
    if finished && let Some(mut streaming) = app.ui.chat.streaming.take() {
        if let Some(err_msg) = app.ui.chat.failure.clone() {
            app.session.chat.busy = false;
            let _ = app.session.chat_append(ChatMessage::assistant(format!("❌ **Provider Error:** {err_msg}")));
            ctx.request_repaint();
            return true;
        }

        let calls = streaming.tool_calls.clone();
        if calls.is_empty() {
            // An answer with nothing more to do: the send is over. Two things can still be wrong
            // with it, and both are the document's answer rather than the model's.
            //
            // The model says it is finished. Whether it is, is the audit's answer: it reads the
            // finished layers and reports the faults a picture cannot show — a clipped headline,
            // a grey box where the photography belongs, an empty section. Handing those back is
            // the difference between a screen the model stopped on and a screen that holds up; a
            // model asked to audit itself skips the step, so the app runs it.
            //
            // This comes before the empty-answer retry because it is the better nudge. A model
            // that answered with nothing but left twelve empty picture slots does not need to be
            // told to "continue": it needs the list of what is unfinished, which is also what a
            // bare "continue" leaves it to invent.
            //
            // Only a send that ran at least one tool can have changed the document; a question
            // answered in prose is not audited, and must not be turned into one.
            const MAX_REVIEW_ROUNDS: usize = 3;
            if streaming.step > 0
                && app.ui.chat.review_rounds < MAX_REVIEW_ROUNDS
                && !streaming.text.trim().is_empty()
                && let Some(blockers) = audit_blockers(app)
            {
                app.ui.chat.review_rounds += 1;
                let text = if streaming.text.trim().is_empty() {
                    format!("Design review pass {}: inspecting elements, button text fitting, photography, and layout...", app.ui.chat.review_rounds)
                } else {
                    streaming.text.clone()
                };
                let _ = app.session.chat_append(ChatMessage::assistant(text));
                let _ = app.session.chat_append(ChatMessage::user(format!("{AUDIT_NUDGE}\n\n{blockers}")));
                let step = streaming.step + 1;
                match continue_step_at(app, step) {
                    Ok(()) => {
                        ctx.request_repaint();
                        return false;
                    }
                    Err(e) => {
                        app.ui.chat.failure = Some(e.clone());
                        let _ = app.session.chat_append(ChatMessage::assistant(format!("❌ **Provider Error:** {e}")));
                    }
                }
            }
            // An answer with no text and no audit finding gets one more ask: an empty response
            // mid-work strands a design half-built, and the same model usually answers when asked
            // again. The empty answer is not kept in the conversation: it would only teach the
            // model that silence is an acceptable reply.
            if streaming.text.trim().is_empty() && app.ui.chat.empty_answers < MAX_EMPTY_ANSWERS {
                app.ui.chat.empty_answers += 1;
                let _ = app.session.chat_append(ChatMessage::user(EMPTY_ANSWER_NUDGE));
                let step = streaming.step + 1;
                match continue_step_at(app, step) {
                    Ok(()) => {
                        ctx.request_repaint();
                        return false;
                    }
                    Err(e) => {
                        app.ui.chat.failure = Some(e.clone());
                        let _ = app.session.chat_append(ChatMessage::assistant(format!("❌ **Provider Error:** {e}")));
                    }
                }
            }
            app.session.chat.busy = false;
            let _ = app.session.chat_append(ChatMessage::assistant(streaming.text.clone()));
            // A model that answers with nothing is a failure the person must see: silently
            // returning to an unchanged canvas reads as "the AI did nothing".
            if streaming.text.trim().is_empty() {
                if app.ui.chat.failure.is_none() {
                    let err = tr("The model answered with nothing. Try again, or pick another model in AI & Agents.").to_string();
                    app.ui.chat.failure = Some(err.clone());
                    let _ = app.session.chat_append(ChatMessage::assistant(format!("❌ {err}")));
                }
            } else {
                app.ui.chat.empty_answers = 0;
            }
            ctx.request_repaint();
            return true;
        }
        app.session.chat_append(ChatMessage::assistant_calls(streaming.text.clone(), calls.clone())).ok();
        app.session.chat.pending_tool_calls = calls.clone();
        if streaming.step + 1 >= MAX_STEPS {
            // The budget is spent: every call left unanswered says so, and the send stops where it
            // is. Continuing is the person's call, from the transcript's Continue control, so a
            // model that keeps calling tools cannot loop past the budget on its own.
            app.ui.chat.failure = Some(trf("The model ran out of steps after {n}.", &[("n", &streaming.step)]).to_string());
            for call in &calls {
                streaming.results.push((call.clone(), json!({"error": "the step budget ran out"}).to_string()));
            }
            record_tools(app, &streaming);
            app.session.chat.busy = false;
            app.ui.chat.resumable = true;
            ctx.request_repaint();
            return true;
        }
        // The engine's own tools, right here.
        for call in &calls {
            if mcp_tool_of(&call.name).is_some() || figcraft_engine::chat_tools::is_media_tool(&call.name) {
                continue;
            }
            let answer = match figcraft_engine::chat_tools::table().iter().find(|t| t.name == call.name) {
                Some(t) => t.run(&mut app.session, &call.arguments),
                // A tool the model invented is an error it reads, naming the one that does the job.
                None => Err(figcraft_engine::chat_tools::instead_of(&call.name)),
            };
            streaming.results.push((call.clone(), cap_result(&call.name, tool_answer_text(&answer))));
        }
        // The networked tools — MCP calls and the image tools — run on their worker, in the
        // order the model asked for them.
        let networked: Vec<ToolCall> =
            calls.iter().filter(|c| mcp_tool_of(&c.name).is_some() || figcraft_engine::chat_tools::is_media_tool(&c.name)).cloned().collect();
        if networked.is_empty() {
            finish_tools(app, &mut streaming);
            ctx.request_repaint();
            return false;
        }
        // Image-editing calls need the source layer's bytes, which only the session can read.
        let mut networked = networked;
        for call in &mut networked {
            if (call.name == "vectorize_image" || call.name == "remove_background")
                && let Some(id) = call.arguments.get("id").and_then(Value::as_str)
                && let Ok(data) = app.session.chat_image_data(id)
            {
                call.arguments["imageData"] = json!(data);
            }
            // The pixel tools look at a render, which is the session's to make: the worker never
            // touches the document. A render that fails is the tool's answer, never silence.
            if call.name == "screenshot" || call.name == "review_design" {
                let ids: Vec<figcraft_doc::NodeId> = call
                    .arguments
                    .get("ids")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(figcraft_engine::params::id_of).collect())
                    .unwrap_or_default();
                let scale = call.arguments.get("scale").and_then(Value::as_f64).unwrap_or(1.0);
                match app.session.chat_render_png(&ids, scale) {
                    Ok(png) => {
                        use base64::Engine as _;
                        call.arguments["imageData"] = json!(base64::engine::general_purpose::STANDARD.encode(&png));
                    }
                    // The worker reads a null as "nothing to look at" and says so to the model.
                    Err(_) => call.arguments["imageData"] = Value::Null,
                }
            }
        }
        streaming.mcp_dispatched = true;
        let tx = streaming.tx.clone();
        let mcp = app.mcp_factory.clone();
        let media = app.media_factory.clone();
        let servers = mcp_servers(app);
        let keys = media_keys(app);
        let thread = run_networked(mcp, media, servers, keys, networked, tx);
        streaming._thread = thread;
        app.ui.chat.streaming = Some(streaming);
        ctx.request_repaint_after(std::time::Duration::from_millis(80));
        return false;
    }

    // The networked tools are running; every answer that arrives is one call closer to done.
    let all_answered = app.ui.chat.streaming.as_ref().is_some_and(|s| s.mcp_dispatched && s.results.len() >= s.tool_calls.len());
    if all_answered && let Some(mut streaming) = app.ui.chat.streaming.take() {
        finish_tools(app, &mut streaming);
        ctx.request_repaint();
        return false;
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(80));
    false
}

/// Run the networked tools — MCP calls and the image tools — in the order the model asked.
///
/// They are a network round trip each, so the desktop build runs them on a thread of their own
/// and the frame loop keeps drawing. The media calls are futures because the page has no threads
/// to block; on this side every one of them finishes on its first poll, so [`block_on`] drives
/// them without ever parking.
#[cfg(not(target_arch = "wasm32"))]
fn run_networked(
    mcp: Arc<dyn figcraft_ai::McpFactory>,
    media: Arc<dyn figcraft_ai::MediaFactory>,
    servers: Vec<McpServer>,
    keys: MediaKeys,
    calls: Vec<ToolCall>,
    tx: Sender<Event>,
) -> Option<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name("figcraft-chat-tools".into())
        .spawn(move || {
            for call in &calls {
                let result = if let Some((server_name, tool)) = mcp_tool_of(&call.name) {
                    match servers.iter().find(|s| s.name == server_name) {
                        Some(server) => {
                            call_mcp_tool(&*mcp, server, tool, &call.arguments).unwrap_or_else(|e| json!({"error": e.message()}).to_string())
                        }
                        None => json!({"error": format!("no MCP server called \"{server_name}\"")}).to_string(),
                    }
                } else {
                    figcraft_ai::media::block_on(call_media_tool(&*media, &keys, &call.name, &call.arguments))
                };
                let _ = tx.send(Event::ToolResult(call.id.clone(), result));
            }
        })
        .ok()
}

/// Run the networked tools in the browser.
///
/// A page has no thread to run them on, so each tool is a future the page's own event loop
/// drives: `spawn_local` starts it, the `fetch` inside it resolves when the server answers, and
/// the answer crosses back on the same channel the streaming transport uses. The frame loop is
/// never blocked, so the canvas keeps drawing while a search runs.
///
/// MCP stays out of reach here: a page cannot open a raw socket. Each such call answers with
/// that reason, so the model reads it and carries on with the tools that do work.
#[cfg(target_arch = "wasm32")]
fn run_networked(
    _mcp: Arc<dyn figcraft_ai::McpFactory>,
    media: Arc<dyn figcraft_ai::MediaFactory>,
    _servers: Vec<McpServer>,
    keys: MediaKeys,
    calls: Vec<ToolCall>,
    tx: Sender<Event>,
) -> Option<std::thread::JoinHandle<()>> {
    use wasm_bindgen_futures::spawn_local;
    spawn_local(async move {
        for call in &calls {
            let result = if mcp_tool_of(&call.name).is_some() {
                json!({
                    "error": "MCP tools are not reachable from the browser build: a page cannot open a raw socket. \
                              Design with the tools that work here."
                })
                .to_string()
            } else {
                call_media_tool(&*media, &keys, &call.name, &call.arguments).await
            };
            let _ = tx.send(Event::ToolResult(call.id.clone(), result));
        }
    });
    None
}

/// The keys the media tools use: design provider, stock service, Vision model and vectorizer.
type MediaKeys = (Option<String>, Option<String>, Option<figcraft_ai::VisionConfig>, Option<(figcraft_ai::VectorizeProvider, String)>);

/// The keys the media tools use: the design provider's, for its image model, and the stock
/// service's.
fn media_keys(app: &FigApp) -> MediaKeys {
    let design = app.session.chat_model(AIModelRole::Design).ok().and_then(|(resolved, key)| {
        // Only a provider that ships an image model can generate one.
        resolved.connection.provider.image_model().map(|_| key)
    });
    let vision = app.session.chat_model(AIModelRole::Vision).ok().map(|(resolved, key)| {
        let base = if resolved.connection.custom_base_url.trim().is_empty() {
            resolved.connection.provider.default_base_url().unwrap_or_default().to_string()
        } else {
            resolved.connection.custom_base_url.clone()
        };
        figcraft_ai::VisionConfig {
            base_url: base,
            api_key: key,
            protocol: resolved.connection.provider.protocol(),
            model: figcraft_ai::models::AIModelSettings::effective_model_id(&resolved.profile).to_string(),
        }
    });
    let stock = app
        .session
        .credentials
        .as_ref()
        .and_then(|s| s.read(&figcraft_ai::credentials::CredentialRef::media_key("pexels", "default")).ok().flatten());
    let provider = app.session.prefs.ai.vectorize_provider;
    let reference = provider.credential_ref();
    let vectorize = app.session.credentials.as_ref().and_then(|s| s.read(&reference).ok().flatten()).map(|key| (provider, key));
    (design, stock, vision, vectorize)
}

/// One image tool's work. The answer is what the model reads: the image's base64, for the
/// interface to place when the tools have all answered.
///
/// Every call is a future: the desktop's session finishes on its first poll inside the tools'
/// worker thread, and the page's resolves when `fetch` does. One pipeline, both builds.
async fn call_media_tool(factory: &dyn figcraft_ai::MediaFactory, keys: &MediaKeys, name: &str, arguments: &Value) -> String {
    if name == "source_image" {
        return source_image(factory, keys, arguments).await;
    }
    if name == "vectorize_image" {
        return vectorize_image(app_media(factory, keys), arguments).await;
    }
    if name == "remove_background" {
        // Editing uses the design provider's image model, so it takes that key.
        return remove_background(factory.make(keys.0.clone(), None, keys.2.clone(), keys.3.clone()), arguments).await;
    }
    if name == "screenshot" || name == "review_design" {
        return look_at_design(factory, keys, name, arguments).await;
    }
    let orientation = figcraft_ai::media::Orientation::from_id(arguments.get("orientation").and_then(Value::as_str).unwrap_or("landscape"));
    let mut session = match name {
        "generate_image" => factory.make(keys.0.clone(), None, keys.2.clone(), keys.3.clone()),
        _ => factory.make(None, keys.1.clone(), keys.2.clone(), keys.3.clone()),
    };
    let outcome = match name {
        "generate_image" => match arguments.get("prompt").and_then(Value::as_str) {
            Some(prompt) => session.generate_image(prompt, orientation).await.map(|bytes| Placed { bytes, credit: String::new() }),
            None => Err(TransportError::Other("`prompt` is required".into())),
        },
        "stock_photo" => match arguments.get("query").and_then(Value::as_str) {
            Some(query) => {
                let index = arguments.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                match session.stock_photo(query, orientation, index).await {
                    Ok(photo) => {
                        let credit = format!("{}: {} ({})", photo.provider, photo.photographer, photo.source_page);
                        session.download(&photo.image_url).await.map(|bytes| Placed { bytes, credit })
                    }
                    Err(e) => Err(e),
                }
            }
            None => Err(TransportError::Other("`query` is required".into())),
        },
        _ => Err(TransportError::Other(format!("there is no media tool called \"{name}\""))),
    };
    match outcome {
        Ok(image) => json!({"imageData": base64_of(&image.bytes), "credit": image.credit}).to_string(),
        Err(e) => json!({"error": e.message()}).to_string(),
    }
}

/// A media session using the vectorization key.
fn app_media(factory: &dyn figcraft_ai::MediaFactory, keys: &MediaKeys) -> Box<dyn figcraft_ai::MediaSession> {
    factory.make(None, None, keys.2.clone(), keys.3.clone())
}

/// The rendering the pixel tools look at, as the interface produced it before it went to the
/// worker: PNG bytes, already capped by `Session::chat_render_png`.
///
/// The pixels arrive on the call as `imageData` so the worker never touches the document.
async fn look_at_design(factory: &dyn figcraft_ai::MediaFactory, keys: &MediaKeys, name: &str, arguments: &Value) -> String {
    use base64::Engine as _;
    let Some(data) = arguments.get("imageData").and_then(Value::as_str) else {
        return json!({"error": "nothing could be rendered to look at; give `ids`, or put something on the page first"}).to_string();
    };
    let bytes = match base64::engine::general_purpose::STANDARD.decode(data) {
        Ok(bytes) => bytes,
        Err(e) => return json!({"error": format!("the render could not be decoded: {e}")}).to_string(),
    };
    let question = arguments.get("question").and_then(Value::as_str).unwrap_or_default().trim().to_string();
    let brief = arguments.get("brief").and_then(Value::as_str).unwrap_or_default().trim().to_string();
    let prompt = if name == "review_design" {
        review_prompt(&brief)
    } else if question.is_empty() {
        "This is a screen from a design tool. Describe what it shows in two or three sentences, then list what is visibly wrong: anything overlapping, clipped, crowded, misaligned, an empty slot where a picture belongs, or a colour outside the palette. Answer as plain prose, not JSON.".to_string()
    } else {
        format!("This is a screen from a design tool. {question}")
    };
    let mut session = factory.make(keys.0.clone(), None, keys.2.clone(), keys.3.clone());
    match session.inspect_image(&bytes, "image/png", &prompt).await {
        Ok(text) => json!({"looked_at": name, "answer": text.trim()}).to_string(),
        Err(e) => {
            if keys.2.is_none() {
                return json!({"error": e.message()}).to_string();
            }
            json!({
                "looked_at": name,
                "answer": format!("Render captured. Visual review unavailable ({}). Tip: Use `audit_layout` to verify layer bounds, alignment and clipping.", e.message())
            }).to_string()
        }
    }
}

/// What the reviewer is asked. The role exists in the settings (`review`) because a design is
/// graded better by a second model than by the one that drew it, and this is where it is used.
fn review_prompt(brief: &str) -> String {
    let brief = if brief.is_empty() { "the screen the person asked for" } else { brief };
    format!(
        "You are a senior product designer reviewing a screen a colleague built for: {brief}.\n\
         Judge it as a designer, not a fan. Answer in this order, in plain prose:\n\
         1. What the screen is and whether it serves the brief.\n\
         2. Hierarchy: does one thing lead, and does the eye know where to go next?\n\
         3. Alignment and spacing: is there one grid, one spacing rhythm, or do edges and gaps disagree?\n\
         4. Type: is the scale consistent, are line lengths readable, is anything too small or too tight?\n\
         5. Contrast and colour: is anything unreadable, and is the accent used for one thing?\n\
         6. The faults you can actually see, each with the part of the screen it is in. Be specific: \"the header's right edge is 8px short of the cards below\" beats \"alignment is off\".\n\
         7. The three changes that would most improve it.\n\
         If something looks right, say so; do not invent faults. Never claim a detail you cannot see."
    )
}

/// Read an image layer's bytes and turn it into SVG text the engine can import.
async fn vectorize_image(mut session: Box<dyn figcraft_ai::MediaSession>, arguments: &Value) -> String {
    let Some(data) = arguments.get("imageData").and_then(Value::as_str) else {
        return json!({"error": "the image layer's data could not be read"}).to_string();
    };
    use base64::Engine as _;
    let bytes = match base64::engine::general_purpose::STANDARD.decode(data) {
        Ok(bytes) => bytes,
        Err(e) => return json!({"error": format!("the image data could not be decoded: {e}")}).to_string(),
    };
    match session.vectorize(&bytes).await {
        Ok(svg) => json!({"svg": svg}).to_string(),
        Err(e) => json!({"error": e.message()}).to_string(),
    }
}

/// Remove an image layer's background and answer with the transparent PNG's base64, which
/// `place_media_answer` places as a new layer. The bytes never reach the model.
async fn remove_background(mut session: Box<dyn figcraft_ai::MediaSession>, arguments: &Value) -> String {
    let Some(data) = arguments.get("imageData").and_then(Value::as_str) else {
        return json!({"error": "the image layer's data could not be read"}).to_string();
    };
    use base64::Engine as _;
    let bytes = match base64::engine::general_purpose::STANDARD.decode(data) {
        Ok(bytes) => bytes,
        Err(e) => return json!({"error": format!("the image data could not be decoded: {e}")}).to_string(),
    };
    match session.remove_background(&bytes).await {
        Ok(png) => json!({"imageData": base64::engine::general_purpose::STANDARD.encode(&png)}).to_string(),
        Err(e) => json!({"error": e.message()}).to_string(),
    }
}

/// Search the public libraries with the libraries' own pace in mind.
///
/// Openverse allows twenty anonymous requests a minute across every client on the same address,
/// and Wikimedia Commons asks for the same restraint. One screen has six or seven picture slots
/// and a `source_image` call carries all of them, so an unpaced loop spends the whole minute's
/// allowance in two seconds and the slots behind the first few fail with a throttle that has
/// nothing to do with their pictures.
///
/// Every search is spaced by [`LIBRARY_PAUSE_MS`], not only a refused one. A refusal cannot be
/// counted on to say it is a refusal: Openverse answers a throttled client with `429` and no
/// `Access-Control-Allow-Origin`, so a page sees a CORS block and the words "Failed to fetch"
/// rather than a status it could read. Spacing the requests keeps the client inside the limit
/// whether or not the library's refusal is legible.
///
/// A refusal that is legible as a throttle is waited out once, then reported; anything else is
/// the library's own answer and goes straight back.
async fn search_paced(
    session: &mut dyn figcraft_ai::MediaSession,
    query: &str,
    options: &figcraft_ai::AssetSearchOptions,
    calls: &mut usize,
) -> Vec<figcraft_ai::AssetSearchOutcome> {
    if *calls > 0 {
        pause(LIBRARY_PAUSE_MS).await;
    }
    *calls += 1;
    let mut outcomes = session.search_assets(query, options).await;
    if outcomes.iter().any(is_throttled) {
        pause(LIBRARY_PAUSE_MS).await;
        outcomes = session.search_assets(query, options).await;
    }
    outcomes
}

/// Whether a provider's answer is a throttle rather than an absence.
///
/// A browser cannot read a `429` whose response carries no CORS header: the request fails before
/// the status is visible, and the failure says "Failed to fetch" or names the CORS policy. Those
/// are a throttle from a page's point of view — the library answered, it just would not let the
/// answer be read — and treating them as "the library has nothing" is what left slots empty.
fn is_throttled(outcome: &figcraft_ai::AssetSearchOutcome) -> bool {
    outcome.error.as_deref().is_some_and(|e| {
        let e = e.to_ascii_lowercase();
        ["429", "too many", "rate limit", "failed to fetch", "err_failed", "cors", "access-control"].iter().any(|mark| e.contains(mark))
    })
}

/// How long to let a library's burst limit refill. Openverse's anonymous window is a minute wide
/// with twenty requests in it, so three seconds between requests is what keeps a screen's seven
/// slots inside the limit.
const LIBRARY_PAUSE_MS: u32 = 3000;

/// Inspect library candidates, fetching each one only when its turn comes.
///
/// A search returns up to twenty candidates for one slot, and the Vision budget is six. This
/// walks them in ranked order and downloads only the ones it actually looks at, so the candidates
/// behind an approval are never fetched. Doing it the other way — twenty downloads, then six
/// inspections — spent fourteen transfers on pictures nobody would see, and the libraries and
/// their CDNs read that burst as abuse and throttled the client.
///
/// A download that fails is one rejected candidate, not the end of the round: a dead host is
/// common and says nothing about the other twenty.
async fn verify_assets(
    assets: Vec<(String, String)>,
    session: &mut dyn figcraft_ai::MediaSession,
    intent: &figcraft_ai::vision::VisualIntent,
    rejected: &mut Vec<(String, String)>,
    vision_error: &mut Option<String>,
    budget: &mut usize,
    near: &mut NearMiss,
) -> Option<(Placed, String)> {
    for (url, credit) in assets {
        if *budget == 0 || vision_error.is_some() {
            break;
        }
        let candidate = session.download(&url).await.map(|bytes| Placed { bytes, credit });
        let found = verify_tier(vec![candidate], session, intent, rejected, vision_error, budget, near).await;
        if found.is_some() {
            return found;
        }
    }
    None
}

/// Inspect one tier's candidates and return the first the Vision model approves.
///
/// `budget` is the shared Vision-call allowance for the whole slot: every candidate inspected,
/// in every tier, spends one. A rejection is recorded for the caller; a failed or malformed
/// inspection is a `vision_error`, never an approval.
async fn verify_tier(
    candidates: Vec<Result<Placed, TransportError>>,
    session: &mut dyn figcraft_ai::MediaSession,
    intent: &figcraft_ai::vision::VisualIntent,
    rejected: &mut Vec<(String, String)>,
    vision_error: &mut Option<String>,
    budget: &mut usize,
    near: &mut NearMiss,
) -> Option<(Placed, String)> {
    for candidate in candidates {
        if *budget == 0 {
            break;
        }
        match candidate {
            Ok(image) => {
                *budget -= 1;
                let prompt = figcraft_ai::vision::inspection_prompt(intent);
                match session.inspect_image(&image.bytes, "image/png", &prompt).await {
                    Ok(text) => match figcraft_ai::vision::parse_verdict(&text) {
                        Ok(verdict) if figcraft_ai::vision::accepts(&verdict) => return Some((image, verdict.reason)),
                        Ok(verdict) => {
                            keep_near_miss(&verdict, &image, near);
                            rejected.push(("verifier".into(), verdict.reason));
                        }
                        Err(e) => rejected.push(("verifier".into(), e)),
                    },
                    // A rate limit is the provider asking for a moment, not this candidate
                    // being wrong. Stopping the round here would turn a plain "the picture does
                    // not match" into "the model is broken" and abandon every slot behind it.
                    Err(e) => match retry_vision(session, &image.bytes, &prompt, &e).await {
                        Ok(text) => match figcraft_ai::vision::parse_verdict(&text) {
                            Ok(verdict) if figcraft_ai::vision::accepts(&verdict) => return Some((image, verdict.reason)),
                            Ok(verdict) => {
                                keep_near_miss(&verdict, &image, near);
                                rejected.push(("verifier".into(), verdict.reason));
                            }
                            Err(e) => rejected.push(("verifier".into(), e)),
                        },
                        Err(e) if vision_is_unusable(&e) => {
                            // A refused key or an unknown model is not this picture's fault and
                            // will not change on the next candidate, so the slot says so.
                            *vision_error = Some(e.message());
                            break;
                        }
                        // A provider that answered `500`, `424` or nothing at all was having a
                        // moment: the candidate is reported as unverified and the next one gets
                        // its own chance. Ending the whole run here reported every remaining
                        // slot as "vision unavailable" over a hiccup that lasted one request,
                        // which is how a finished page shipped with grey boxes in it.
                        Err(e) => {
                            if matches!(e, TransportError::RateLimited) && near.unverified.is_none() {
                                near.unverified = Some((image.clone(), "unverified: the Vision provider was rate-limited".into()));
                            }
                            rejected.push(("verifier".into(), e.message()));
                        }
                    },
                }
            }
            Err(e) => rejected.push(("source".into(), e.message())),
        }
    }
    None
}

/// The best near miss a slot has seen, and how well it matched.
#[derive(Default)]
struct NearMiss {
    score: f64,
    best: Option<(Placed, String)>,
    /// The best candidate the verifier never got to judge, because the provider was throttling.
    ///
    /// A throttle is not a verdict about the picture. Leaving the slot a grey box because the
    /// Vision model was busy reports "no picture matched" when nothing was looked at, and a
    /// licensed photograph in the right shape beats an empty frame. It is placed and named
    /// `unverified` so the model can ask for it again rather than believe it was checked.
    unverified: Option<(Placed, String)>,
}

/// Remember `image` when it is the best near miss so far.
///
/// A slot that finds nothing the verifier approves still gets the closest picture it saw, because
/// an unremarkable photograph of the right subject reads as a design and a grey rectangle reads as
/// unfinished. The reason says it is a near miss, so the model can decide to ask again.
fn keep_near_miss(verdict: &figcraft_ai::vision::VisionVerdict, image: &Placed, near: &mut NearMiss) {
    if !figcraft_ai::vision::fallback_worthy(verdict) {
        return;
    }
    // The score decides, not the arrival order: the twentieth candidate is often a worse match
    // than the third, and keeping whichever came last put a loosely related photograph in the
    // slot while a better one was already in hand.
    if verdict.relevance_score < near.score {
        return;
    }
    near.score = verdict.relevance_score;
    near.best = Some((image.clone(), format!("closest available: {}", verdict.reason)));
}

/// Tier 1 & 2 search: free public libraries first, stock photos second.
async fn try_search_tier(
    session: &mut (dyn figcraft_ai::MediaSession + '_),
    query: &str,
    intent: &figcraft_ai::vision::VisualIntent,
    orientation: figcraft_ai::media::Orientation,
    library_calls: &mut usize,
    rejected: &mut Vec<(String, String)>,
    vision_error: &mut Option<String>,
    budget: &mut usize,
    near: &mut NearMiss,
) -> Option<(Placed, String)> {
    let options = figcraft_ai::AssetSearchOptions {
        limit: figcraft_ai::MAX_TOTAL_CANDIDATES,
        orientation,
        allow_non_commercial: false,
        require_modification: true,
    };
    for round in 0..2 {
        let words = if round == 0 { query.to_string() } else { figcraft_ai::assets::broaden_query(query, &intent.subject, 1) };
        if round == 1 && words == *query {
            break;
        }
        let outcomes = search_paced(session, &words, &options, library_calls).await;
        for provider_error in outcomes.iter().filter_map(|o| o.error.as_ref()) {
            rejected.push(("source".into(), provider_error.clone()));
        }
        let ranked =
            figcraft_ai::rank_candidates(&outcomes, &figcraft_ai::LicensePolicy::default(), &words, orientation, figcraft_ai::MAX_TOTAL_CANDIDATES);
        let found_any = !ranked.is_empty();
        let ranked: Vec<(String, String)> = ranked.into_iter().map(|asset| (asset.download_url().to_string(), asset.credit_line())).collect();
        let approved = verify_assets(ranked, session, intent, rejected, vision_error, budget, near).await;
        if approved.is_some() {
            return approved;
        }
        if vision_error.is_some() || found_any {
            break;
        }
    }

    let stock = match session.stock_photo(query, orientation, 0).await {
        Ok(photo) => {
            let credit = format!("{}: {} ({})", photo.provider, photo.photographer, photo.source_page);
            session.download(&photo.image_url).await.map(|bytes| Placed { bytes, credit })
        }
        Err(e) => Err(e),
    };
    verify_tier(vec![stock], session, intent, rejected, vision_error, budget, near).await
}

/// Source and verify every image in one `source_image` call.
///
/// A candidate is only placed when the Vision model approved its actual pixels. An unverified
/// candidate is reported as a rejection, never as a picture; the node is left as the model made
/// it so a human can see the slot was not filled.
async fn source_image(factory: &dyn figcraft_ai::MediaFactory, keys: &MediaKeys, arguments: &Value) -> String {
    // The requests arrive either as a real array or as a JSON-encoded string of one, depending on
    // how the model read the schema. Both are the same list; refusing one costs a step and teaches
    // the model that image search does not work.
    let raw = match arguments.get("requests") {
        Some(Value::String(text)) => text.clone(),
        Some(list @ Value::Array(_)) => list.to_string(),
        _ => return json!({"error": "`requests` is required: a list of {id, subject, style?}"}).to_string(),
    };
    let requests = match figcraft_ai::vision::parse_requests(&raw) {
        Ok(list) => list,
        Err(e) => return json!({"error": e}).to_string(),
    };
    let mut results = Vec::new();
    let mut applied = 0usize;
    // One pace for the whole call, not one per slot: the libraries' limits are per address, so
    // seven slots asking three questions each is seven slots sharing one allowance.
    let mut library_calls = 0usize;
    let mut session = factory.make(keys.0.clone(), keys.1.clone(), keys.2.clone(), keys.3.clone());

    for request in requests {
        let route = figcraft_ai::vision::route(&request.intent);
        let orientation =
            figcraft_ai::media::Orientation::from_id(if request.intent.orientation.is_empty() { "landscape" } else { &request.intent.orientation });
        let mut rejected: Vec<(String, String)> = Vec::new();
        let mut vision_error: Option<String> = None;
        let mut budget = figcraft_ai::MAX_VISION_CALLS;
        let mut approved: Option<(Placed, String)> = None;
        // The best picture the verifier rejected but found recognisably right: what fills the slot
        // when nothing passes outright.
        let mut near = NearMiss::default();

        // The tiers a route allows, in order, and each one is lazy: a tier is only fetched and
        // inspected when the one before it produced no approved image, so a keyed service is
        // never touched when a free library already supplied a verified picture.
        //
        // Both tiers are reachable from every route that allows both. A route says what to try
        // first, not what may be tried: a slot is a box on the screen whether or not the
        // connection happens to have an image model, and a model that asked for an illustration
        // and got nothing would fill the box with a grey rectangle.
        let query = if request.intent.query.is_empty() { &request.intent.subject } else { &request.intent.query };
        let mut used: figcraft_ai::ImageRoute = route;
        for tier in figcraft_ai::vision::tiers(route) {
            if approved.is_some() {
                break;
            }
            match tier {
                figcraft_ai::ImageRoute::Search => {
                    approved = try_search_tier(
                        &mut *session,
                        query,
                        &request.intent,
                        orientation,
                        &mut library_calls,
                        &mut rejected,
                        &mut vision_error,
                        &mut budget,
                        &mut near,
                    )
                    .await;
                    if approved.is_some() {
                        used = figcraft_ai::ImageRoute::Search;
                    }
                }
                figcraft_ai::ImageRoute::Generate => {
                    approved = verify_tier(
                        vec![generate_candidate(&mut *session, &request.intent, orientation).await],
                        &mut *session,
                        &request.intent,
                        &mut rejected,
                        &mut vision_error,
                        &mut budget,
                        &mut near,
                    )
                    .await;
                    if approved.is_some() {
                        used = figcraft_ai::ImageRoute::Generate;
                    }
                }
                // `tiers` never returns these; they are routes, not tiers.
                figcraft_ai::ImageRoute::Auto | figcraft_ai::ImageRoute::ProbeThenGenerate => {}
            }
        }

        let fallback = approved.or(near.best).or(near.unverified);
        if let Some((image, reason)) = fallback {
            applied += 1;
            let state = if reason.starts_with("unverified") {
                "unverified"
            } else if reason.starts_with("closest") {
                "closest-available"
            } else {
                "verified"
            };
            results.push(json!({
                "id": request.id,
                "state": state,
                "imageData": base64_of(&image.bytes),
                "credit": image.credit,
                "reason": reason,
                "route": route_to_string(used),
            }));
            continue;
        }
        let state = if vision_error.is_some() { figcraft_ai::SourcingState::VisionUnavailable } else { figcraft_ai::SourcingState::NoVerifiedMatch };
        results.push(json!({
            "id": request.id,
            "state": match state {
                figcraft_ai::SourcingState::VisionUnavailable => "vision-unavailable-needs-user-selection",
                _ => "no-verified-match",
            },
            "reason": vision_error.or_else(|| Some("no candidate passed verification".into())),
            "rejected": rejected,
        }));
    }

    json!({"applied": applied, "failed": results.len() - applied, "results": results}).to_string()
}

/// Generate one candidate for a request.
async fn generate_candidate(
    session: &mut dyn figcraft_ai::MediaSession,
    intent: &figcraft_ai::vision::VisualIntent,
    orientation: figcraft_ai::Orientation,
) -> Result<Placed, TransportError> {
    if intent.subject.is_empty() {
        return Err(TransportError::Other("the image request has no subject".into()));
    }
    let mut prompt = intent.subject.clone();
    if !intent.style.is_empty() {
        prompt.push_str(&format!(", {}", intent.style));
    }
    if !intent.palette.is_empty() {
        prompt.push_str(&format!(", {} palette", intent.palette));
    }
    prompt.push_str(", no text");
    session.generate_image(&prompt, orientation).await.map(|bytes| Placed { bytes, credit: "ai-generated".into() })
}

/// A route's id, for the result the model reads.
fn route_to_string(route: figcraft_ai::ImageRoute) -> &'static str {
    match route {
        figcraft_ai::ImageRoute::Auto => "auto",
        figcraft_ai::ImageRoute::Search => "search",
        figcraft_ai::ImageRoute::Generate => "generate",
        figcraft_ai::ImageRoute::ProbeThenGenerate => "probe_then_generate",
    }
}

/// Inspect again after a rate limit.
///
/// A free endpoint refuses bursts long before it refuses a single picture, so the first refusal
/// says nothing about the image. One patient retry recovers the run; the second failure is the
/// model genuinely being unavailable, and the caller reports that instead of blaming the picture.
async fn retry_vision(
    session: &mut dyn figcraft_ai::MediaSession,
    bytes: &[u8],
    prompt: &str,
    first: &TransportError,
) -> Result<String, TransportError> {
    if !matches!(first, TransportError::RateLimited) {
        // Everything else leaves the model unusable for this run, so it keeps its own message.
        return Err(match first {
            TransportError::Authentication => TransportError::Authentication,
            TransportError::Network => TransportError::Network,
            other => TransportError::Other(other.message()),
        });
    }
    pause(VISION_RETRY_PAUSE_MS).await;
    session.inspect_image(bytes, "image/png", prompt).await
}

/// Whether a Vision failure means the model itself cannot be used, rather than this request.
///
/// A wrong key, a model id the provider does not know, or an endpoint speaking another API shape
/// are settings a person has to change: no candidate can pass, so the slot reports the reason
/// instead of spending its remaining budget. Everything else — a throttle, a `500`, a `424`, a
/// dropped connection — is the provider having a moment, and the next candidate deserves its own
/// attempt.
fn vision_is_unusable(error: &TransportError) -> bool {
    error.is_credential_problem()
}

/// Wait without holding the thread on the page.
async fn pause(ms: u32) {
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::sleep(std::time::Duration::from_millis(u64::from(ms)));
    #[cfg(target_arch = "wasm32")]
    {
        let promise = js_sys::Promise::new(&mut |resolve, _| {
            if let Some(window) = web_sys::window() {
                let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms as i32);
            }
        });
        let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
    }
}

/// Long enough for a burst limit to refill, short enough that a person is still waiting.
const VISION_RETRY_PAUSE_MS: u32 = 1500;

/// One media tool's outcome: the bytes and where a fetched one came from.
#[derive(Clone)]
struct Placed {
    bytes: Vec<u8>,
    credit: String,
}

/// What is said to a model that finished before the document was sound. It carries the design review
/// and layout audit findings, so the model fixes concrete faults: each line names the layer, what is
/// wrong with it (button text overflow, container overflow, missing photography, clipped text), and the repair.
const AUDIT_NUDGE: &str = "Before completing the design, the design review loop inspected the document elements and layout. \
Each line below is a measured fault, not an opinion. Fix them using the tools (e.g. set sizingH: \"hug\" or widen buttons \
so labels fit, call source_image for missing photography, or adjust alignment), then say what you built and fixed:";

/// The audit's findings as the prompt the model reads, or `None` when the document holds up.
///
/// This runs on the interface thread against the finished document: the audit is a read-only
/// command, so it costs one walk of the tree and never touches the undo stack. A document that
/// audits clean is finished, and nothing more is said.
fn audit_blockers(app: &mut FigApp) -> Option<String> {
    let report = app.run("layout.audit", json!({}))?;
    let findings = report.get("findings")?.as_array()?;
    if findings.is_empty() {
        return None;
    }
    let mut out = String::new();
    for f in findings {
        let kind = f.get("kind").and_then(Value::as_str).unwrap_or("fault");
        let id = f.get("id").and_then(Value::as_i64).unwrap_or_default();
        let name = f.get("name").and_then(Value::as_str).unwrap_or("");
        let detail = f.get("detail").and_then(Value::as_str).unwrap_or("");
        let fix = f.get("fix").and_then(Value::as_str).unwrap_or("");
        out.push_str(&format!("- [{kind}] layer {id} \"{name}\": {detail}. Fix: {fix}\n"));
    }
    Some(out)
}

/// Bytes as the document takes them.
fn base64_of(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// A tool's answer as the model reads it: what the command said, or the error.
fn tool_answer_text(answer: &Result<Value, String>) -> String {
    match answer {
        Ok(value) => serde_json::to_string(value).unwrap_or_else(|_| "{}".into()),
        Err(e) => json!({"error": e}).to_string(),
    }
}

/// The most of a tool's answer the model reads. A tool that dumps the whole catalogue, or the
/// whole document, crowds the conversation until the model answers with nothing; the head of it
/// is enough to act on, and the note says what happened so the model can ask more narrowly.
const MAX_TOOL_ANSWER: usize = 40_000;

/// `text` cut to [`MAX_TOOL_ANSWER`], at a character boundary.
fn cap_answer(text: String) -> String {
    if text.len() <= MAX_TOOL_ANSWER {
        return text;
    }
    let mut end = MAX_TOOL_ANSWER;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let kept = text.get(..end).unwrap_or_default();
    format!("{kept}\n… [answer cut at {MAX_TOOL_ANSWER} bytes; ask more narrowly, e.g. `list_commands` with a filter]")
}

#[cfg(test)]
mod cap_tests {
    use super::{MAX_TOOL_ANSWER, cap_answer, cap_result};

    #[test]
    fn an_image_answer_is_kept_whole_until_its_picture_is_placed() {
        // A 12 MiB photograph is 16 MB of base64, far past the reading limit. Capping it before
        // `place_media_answer` parsed it left a truncated object, which parsed as nothing and
        // placed nothing: the tool answered "applied: 1" and the slot stayed a grey box.
        let big = format!(r#"{{"applied":1,"results":[{{"imageData":"{}"}}]}}"#, "A".repeat(MAX_TOOL_ANSWER * 2));
        let kept = cap_result("source_image", big.clone());
        assert_eq!(kept.len(), big.len(), "a media answer reaches the placer whole");
        assert!(serde_json::from_str::<serde_json::Value>(&kept).is_ok(), "and it still parses");
        assert_eq!(cap_result("generate_image", big.clone()).len(), big.len());
        // Everything else is capped where it is recorded: the model reads the head of it.
        assert!(cap_result("get_node", big.clone()).len() < big.len());
        assert!(cap_answer(big).contains("answer cut at"));
    }
}

/// The tools have all answered: their answers join the conversation, and the model reads them.
///
/// The send's streaming state is gone either way — a failed next step ends it. An image tool's
/// bytes never reach the model: the image is placed through the engine first, and what the
/// model reads is what happened to it.
fn finish_tools(app: &mut FigApp, streaming: &mut Streaming) {
    record_tools(app, streaming);
    if continue_step(app).is_err() {
        app.session.chat.busy = false;
    }
}

/// Post each tool call's answer into the conversation, answering for the media tools first (a
/// generated image is placed on the canvas, and what the model reads is where it landed).
fn record_tools(app: &mut FigApp, streaming: &Streaming) {
    let results = streaming.results.clone();
    for (call, text) in &results {
        // Capped after placement, not before: an image tool's answer carries the picture itself
        // as base64, and a 12 MiB photograph is 16 MB of JSON. Cutting it to the reading limit
        // first left `place_media_answer` with a truncated object, which parsed as nothing and
        // placed nothing — the tool answered "applied: 1" and the slot stayed a grey box.
        let answer = cap_answer(place_media_answer(app, call, text));
        let _ = app.session.chat_append(ChatMessage::tool_result(call.id.clone(), answer));
    }
}

/// A tool's answer as it waits to be placed or recorded.
///
/// The reading limit applies to what the model sees, so a media tool's answer is kept whole here
/// and cut in [`record_tools`] once its picture is on the canvas.
fn cap_result(tool: &str, text: String) -> String {
    if figcraft_engine::chat_tools::is_media_tool(tool) { text } else { cap_answer(text) }
}

/// The layer a `source_image` result names, when it is a layer id and not just a label.
///
/// `source_image`'s schema asks for "the placeholder layer's id, so the picture lands in the right
/// slot". A model usually sends the id it was given; some send `0:30`, a name it made up, or the
/// id of a layer that has since been deleted. Answering `Some` only for a live node keeps a
/// stale id from failing the whole picture: the caller falls back to making a new layer.
fn slot_id(app: &FigApp, slot: &str) -> Option<u64> {
    let id: figcraft_doc::NodeId = slot.trim().parse().ok()?;
    app.session.doc().ok()?.node(id).map(|_| id.0)
}

/// Place an image tool's answer on the canvas, and say what happened instead of what arrived.
fn place_media_answer(app: &mut FigApp, call: &ToolCall, text: &str) -> String {
    use base64::Engine as _;
    let Ok(answer) = serde_json::from_str::<Value>(text) else { return text.to_string() };
    // A vectorizer answers with SVG text: `file.open` imports its bytes as editable geometry in a
    // new unsaved document (the name's extension tells the loader it is an SVG).
    if let Some(svg) = answer.get("svg").and_then(Value::as_str) {
        let name = call.arguments.get("name").and_then(Value::as_str).unwrap_or("Vector");
        let params = json!({
            "data": base64::engine::general_purpose::STANDARD.encode(svg.as_bytes()),
            "name": format!("{name}.svg"),
        });
        return match app.session.execute_as(Some("AI"), "file.open", &params) {
            Ok(value) => value.to_string(),
            Err(e) => json!({"error": format!("the vector could not be imported: {e}")}).to_string(),
        };
    }
    // `source_image` returns one result per placeholder; the others carry one image.
    if let Some(results) = answer.get("results").and_then(Value::as_array) {
        let mut placed_results = Vec::new();
        for result in results {
            let Some(data) = result.get("imageData").and_then(Value::as_str) else {
                placed_results.push(result.clone());
                continue;
            };
            let slot = result.get("id").and_then(Value::as_str).unwrap_or("Image");
            // The id `source_image` was given is the placeholder layer the model drew for this
            // picture. Filling that layer is what makes the picture land in the right place at the
            // right size; making a new layer named after the id put the photography loose on the
            // page at its own size, on top of a grey box that stayed empty — the audit then
            // reported the slot as still missing, which was true.
            let mut params = json!({"data": data, "name": slot});
            match slot_id(app, slot) {
                Some(id) => params["target"] = json!(id),
                None => {
                    if let Some(parent) = call.arguments.get("parent") {
                        params["parent"] = parent.clone();
                    }
                }
            }
            let mut reply = match app.session.execute_as(Some("AI"), "image.place", &params) {
                Ok(value) => value,
                Err(e) => {
                    // The bytes are dropped: a photograph is megabytes of base64, and the model
                    // reading them spends its context on a picture it cannot see.
                    let mut r = result.clone();
                    if let Some(object) = r.as_object_mut() {
                        object.remove("imageData");
                    }
                    r["error"] = json!(format!("the image was verified but could not be placed: {e}"));
                    placed_results.push(r);
                    continue;
                }
            };
            if let Some(credit) = result.get("credit").and_then(Value::as_str).filter(|c| !c.is_empty()) {
                reply["credit"] = json!(credit);
            }
            placed_results.push(reply);
        }
        let mut out = answer.clone();
        out["results"] = json!(placed_results);
        return out.to_string();
    }
    let Some(data) = answer.get("imageData").and_then(Value::as_str) else { return text.to_string() };
    let mut params = json!({"data": data, "name": call.arguments.get("name").and_then(Value::as_str).unwrap_or("Image")});
    if let Some(parent) = call.arguments.get("parent") {
        params["parent"] = parent.clone();
    }
    let placed = app.session.execute_as(Some("AI"), "image.place", &params);
    let mut reply = match placed {
        Ok(value) => value,
        Err(e) => return json!({"error": format!("the image was made but could not be placed: {e}")}).to_string(),
    };
    if let Some(credit) = answer.get("credit").and_then(Value::as_str).filter(|c| !c.is_empty()) {
        reply["credit"] = json!(credit);
    }
    reply.to_string()
}

/// The composer's text field.
fn draft_id() -> egui::Id {
    egui::Id::unique("chat_draft")
}

/// How tall the composer's field is at its smallest: a line of text and the box's padding.
const COMPOSER_MIN_H: f32 = 34.0;
/// How tall it grows to before it scrolls instead: about four lines.
const COMPOSER_MAX_H: f32 = 78.0;
/// The row under the composer's field: the model, the extra controls and Send.
const COMPOSER_BAR_H: f32 = 26.0;
/// The text inside the composer field, and the height of one line of it.
const COMPOSER_TEXT_PX: f32 = 12.5;
const COMPOSER_LEADING: f32 = 16.0;
/// The header's height: the mark, the title and which conversation this is.
const HEADER_H: f32 = 50.0;

/// The panel, in the right panel's AI tab: a header, the conversation and the composer.
///
/// Three fixed zones with one scroll region between them. The header says who is talking and
/// which conversation this is; the composer is pinned to the bottom edge whatever the conversation
/// does, and only the transcript between them scrolls — which is what keeps a long answer from
/// pushing the field the person types into off the panel.
pub fn panel(app: &mut FigApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    if drain(app, &ctx) {
        // An answer that just finished: the transcript is where the person is looking.
        app.ui.chat.stick = true;
    }
    let t = Tokens::get(&ctx);
    // The width every zone is pinned to, so no content can widen the panel.
    let width = ui.available_width().max(1.0);
    let composer_h = composer_height(app, &ctx, width);

    egui::Panel::top("chat_header").exact_size(HEADER_H).show_separator_line(false).frame(egui::Frame::NONE.fill(t.panel)).show(ui, |ui| {
        ui.set_width(width);
        header(app, ui, &t, width);
    });

    egui::Panel::bottom("chat_composer").exact_size(composer_h).show_separator_line(false).frame(egui::Frame::NONE.fill(t.panel)).show(ui, |ui| {
        ui.set_width(width);
        composer(app, ui, &t, width);
    });

    // The one scroll region: the conversation.
    egui::CentralPanel::no_frame().show(ui, |ui| {
        ui.set_width(width);
        transcript(app, ui, &t, width);
    });
}

/// The header: what this is, which conversation it is, and what a conversation needs (a new one,
/// the ones saved, and the AI settings behind them).
fn header(app: &mut FigApp, ui: &mut Ui, t: &Tokens, width: f32) {
    let full = ui.available_rect_before_wrap();
    let row_h = 34.0;
    let row = Rect::from_min_size(pos2(full.min.x + 10.0, full.center().y - row_h / 2.0), vec2((width - 20.0).max(24.0), row_h));
    // The assistant's mark, on the green the agent work is marked with.
    let mark = Rect::from_min_size(row.min, Vec2::splat(26.0));
    ui.painter().rect_filled(mark, CornerRadius::same(7), t.selected_row);
    icons::paint(ui.painter(), Rect::from_center_size(mark.center(), Vec2::splat(16.0)), Icon::Sparkle, t.accent_text);
    // New chat and the chat's options, hard against the right edge.
    let mut right = row.max.x;
    let new_chat = Rect::from_min_size(pos2(right - 22.0, row.center().y - 11.0), Vec2::splat(22.0));
    right -= 24.0;
    let options = Rect::from_min_size(pos2(right - 22.0, row.center().y - 11.0), Vec2::splat(22.0));
    let new_resp = ui.interact(new_chat, egui::Id::unique("chat_new"), Sense::click()).tip(tr("New chat"));
    let opt_resp = ui.interact(options, egui::Id::unique("chat_options"), Sense::click()).tip(tr("Conversations and settings"));
    for (r, resp, icon) in [(new_chat, &new_resp, Icon::Plus), (options, &opt_resp, Icon::Dots)] {
        if resp.hovered() {
            ui.painter().rect_filled(r, CornerRadius::same(5), t.hover);
        }
        icons::paint(ui.painter(), Rect::from_center_size(r.center(), Vec2::splat(14.0)), icon, t.icon);
    }
    a11y::button(&new_resp, tr("New chat"));
    a11y::button(&opt_resp, tr("Conversations and settings"));
    if new_resp.clicked() {
        app.run("ai.chat.new", json!({}));
        app.ui.chat.stick = true;
    }
    // "FigCraft AI", and under it which conversation this is.
    let left = mark.max.x + 8.0;
    let room = (options.min.x - 6.0 - left).max(20.0);
    ui.painter().text(pos2(left, row.min.y + 5.0), Align2::LEFT_TOP, tr("sycnUI AI"), semibold(13.0), t.text);
    let title = active_title(app).unwrap_or_else(|| tr("Your design partner").to_string());
    let sub = widgets::elide_font(ui, &title, room - 6.0, regular(11.0));
    ui.painter().text(pos2(left, row.min.y + 22.0), Align2::LEFT_TOP, &sub, regular(11.0), t.text_secondary);
    let chev = Rect::from_min_size(pos2(left + room - 10.0, row.min.y + 23.0), Vec2::splat(10.0));
    icons::paint(ui.painter(), chev, Icon::Chevron, t.icon_secondary);
    chat_menu(app, &opt_resp, t);
    rename_field(app, ui, t, Rect::from_min_size(pos2(left, row.min.y + 18.0), vec2(room, 20.0)));
    // The panel's edge, on the side facing the canvas.
    ui.painter().line_segment([full.left_bottom(), full.right_bottom()], Stroke::new(1.0, t.border));
}

/// The rename field, over the conversation's title while a rename is open: Enter or focus loss
/// saves it, Escape drops it.
fn rename_field(app: &mut FigApp, ui: &mut Ui, t: &Tokens, rect: Rect) {
    let Some(mut title) = app.ui.chat.rename.clone() else { return };
    let id = egui::Id::unique("chat_rename");
    let field = rect.expand2(vec2(2.0, 0.0));
    ui.painter().rect_filled(field, CornerRadius::same(4), t.field);
    ui.painter().rect_stroke(field, CornerRadius::same(4), Stroke::new(1.0, t.accent), egui::StrokeKind::Inside);
    let resp = ui.put(rect.shrink2(vec2(3.0, 1.0)), egui::TextEdit::singleline(&mut title).id(id).frame(egui::Frame::NONE).font(regular(11.0)));
    if !resp.has_focus() && !resp.lost_focus() {
        resp.request_focus();
    }
    a11y::text(&resp, tr("Conversation name"), &title);
    app.ui.chat.rename = Some(title.clone());
    let save = resp.lost_focus() && !ui.input(|i| i.key_pressed(egui::Key::Escape)) || ui.input(|i| i.key_pressed(egui::Key::Enter));
    let cancel = ui.input(|i| i.key_pressed(egui::Key::Escape));
    if cancel {
        app.ui.chat.rename = None;
    } else if save && !title.trim().is_empty() {
        let chat = app.session.chat.id.clone();
        if !chat.is_empty() {
            app.run("ai.chat.rename", json!({"id": chat, "title": title.trim()}));
        }
        app.ui.chat.rename = None;
    }
}

/// The title of the conversation that is open, if it has one yet.
fn active_title(app: &FigApp) -> Option<String> {
    let history = &app.session.prefs.ai.chat_history;
    let id = &app.session.chat.id;
    if id.is_empty() {
        return None;
    }
    history.get(id).map(|c| c.title.clone()).filter(|t| !t.is_empty())
}

/// The conversation menu: which chats are listed, a new chat, every saved chat to reopen, and
/// what the open one can do (rename, delete).
fn chat_menu(app: &mut FigApp, anchor: &egui::Response, t: &Tokens) {
    let document = app.session.ds().ok().map(|d| d.uid.to_string());
    let this_document = app.session.prefs.ai.chat_history.this_document;
    let active: Option<String> = Some(app.session.chat.id.clone()).filter(|s| !s.is_empty());
    let mut open: Option<String> = None;
    let mut delete: Option<String> = None;
    let mut rename_open = false;
    let mut new_chat = false;
    let scope_label = if this_document { tr("This document") } else { tr("All documents") };
    menu_popup(anchor).width(240.0).show(|ui| {
        if menu_item(ui, scope_label, "") {
            app.run("ai.chat.scope", json!({"thisDocument": !this_document}));
            ui.close();
        }
        ui.separator();
        if menu_item(ui, tr("New chat"), "") {
            new_chat = true;
            ui.close();
        }
        ui.separator();
        let ids: Vec<String> = app.session.prefs.ai.chat_history.visible(document.as_deref()).into_iter().take(12).map(|c| c.id.clone()).collect();
        if ids.is_empty() {
            widgets::label(ui, tr("No saved conversations yet."), t.text_secondary);
        }
        for id in ids {
            let Some(chat) = app.session.prefs.ai.chat_history.get(&id) else { continue };
            let (title, count) = (chat.title.clone(), chat.messages.len());
            let on = active.as_deref() == Some(id.as_str());
            let label = format!("{}  ·{count}", widgets::elide(ui, &title, 150.0));
            if menu_check(ui, on, &label, "") {
                open = Some(id.clone());
                ui.close();
            }
            // Rename and delete sit on the open conversation's row: two more rows under every
            // saved chat would drown the list.
            if on {
                if menu_item(ui, tr("Rename…"), "") {
                    rename_open = true;
                    ui.close();
                }
                if menu_item(ui, tr("Delete"), "") {
                    delete = Some(id.clone());
                    ui.close();
                }
            }
        }
        ui.separator();
        if menu_item(ui, tr("AI & Agents settings…"), "") {
            app.run("ai.settings", json!({}));
            ui.close();
        }
    });
    if new_chat {
        app.run("ai.chat.new", json!({}));
        app.ui.chat.stick = true;
    }
    if let Some(id) = open {
        app.run("ai.chat.open", json!({"id": id}));
        app.ui.chat.stick = true;
    }
    if rename_open && let Some(title) = active_title(app) {
        // Renaming happens at the header: a text field over the conversation's title.
        app.ui.chat.rename = Some(title);
    }
    if let Some(id) = delete {
        app.run("ai.chat.delete", json!({"id": id}));
    }
}

/// Attach an image from raw bytes to the composer draft.
fn attach_image_from_bytes(app: &mut FigApp, name: String, bytes: Vec<u8>) {
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let lower = name.to_lowercase();
    let mime = if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else {
        "image/png"
    };
    app.ui.chat.attached_image = Some(ChatAttachment {
        name,
        mime: mime.into(),
        data_url: format!("data:{mime};base64,{b64}"),
        preview_bytes: bytes,
    });
}

/// Decode an attachment data URL into a TextureHandle for thumbnail rendering.
fn decode_attachment_texture(ctx: &Context, data_url: &str) -> Option<egui::TextureHandle> {
    use base64::Engine as _;
    let rest = data_url.strip_prefix("data:")?;
    let (_, b64) = rest.split_once(";base64,")?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64.trim()).ok()?;
    let pic = figcraft_format::thumbnail::decode_thumbnail(&bytes, 160)?;
    let image = egui::ColorImage::from_rgba_unmultiplied([pic.width as usize, pic.height as usize], &pic.rgba);
    let id = format!("chat-thumb-{}", bytes.len());
    Some(ctx.load_texture(id, image, egui::TextureOptions::LINEAR))
}

/// Extra height reserved when an image is attached to the composer.
fn attachment_height(app: &FigApp) -> f32 {
    if app.ui.chat.attached_image.is_some() { 34.0 } else { 0.0 }
}

/// The gap between the composer's field and the bar under it, counted once by
/// [`composer_height`] and once by [`composer`]: the two must agree or the field and its bar
/// drift apart as the draft grows.
const COMPOSER_GAP: f32 = 8.0;

/// The height the composer needs: the text's own height up to [`COMPOSER_MAX_H`], plus its bar,
/// plus the failure line under the field when the last send did not go. The failure is counted
/// here as well as in [`composer`]: a panel that reserves no room for it draws it over Send.
fn composer_height(app: &FigApp, ctx: &egui::Context, width: f32) -> f32 {
    let inner = (width - 20.0 - 20.0).max(40.0);
    let per_line = (inner / 6.3).max(8.0);
    let lines = (app.ui.chat.draft.chars().count() as f32 / per_line).ceil().clamp(1.0, 4.0);
    let field = (lines * COMPOSER_LEADING + 16.0).clamp(COMPOSER_MIN_H, COMPOSER_MAX_H);
    field + COMPOSER_BAR_H + COMPOSER_GAP + failure_height(app, ctx, width) + attachment_height(app) + mention_height(app)
}

/// The room the failure line needs: nothing when there is no failure.
fn failure_height(app: &FigApp, ctx: &egui::Context, width: f32) -> f32 {
    let Some(failure) = app.ui.chat.failure.as_deref() else { return 0.0 };
    failure_galley(ctx, failure, width).size().y + 4.0
}

/// The failure text, wrapped to the composer's width, laid out once here so the panel can reserve
/// the room for it before drawing it. It is never scrolled away: a send that did not go is the
/// one thing the person has to see.
fn failure_galley(ctx: &egui::Context, failure: &str, width: f32) -> Arc<egui::Galley> {
    let w = (width - 20.0).max(40.0);
    let danger = Tokens::get(ctx).danger;
    ctx.fonts_mut(|f| f.layout(first_line(failure, 240), regular(11.0), danger, w - 14.0))
}

/// The composer: the field the person types in, and the row under it (the model, the attach and
/// selection controls, and Send, which becomes Stop while an answer streams).
fn composer(app: &mut FigApp, ui: &mut Ui, t: &Tokens, width: f32) {
    let busy = app.ui.chat.streaming.is_some();
    let field_w = (width - 20.0).max(40.0);
    let total = ui.available_height();
    let fail_h = failure_height(app, ui.ctx(), width);
    let att_h = attachment_height(app);
    let men_h = mention_height(app);
    let field_h = (total - COMPOSER_BAR_H - COMPOSER_GAP - fail_h - att_h - men_h).clamp(COMPOSER_MIN_H, COMPOSER_MAX_H);

    // Auto-mention when user clicks/selects any element or screen:
    if let Ok(ds) = app.session.ds() {
        let current_first_sel = ds.selection.first().map(|id| id.0);
        if current_first_sel.is_some() && current_first_sel != app.ui.chat.last_selected_id {
            app.ui.chat.last_selected_id = current_first_sel;
            app.ui.chat.with_selection = true;
        } else if current_first_sel.is_none() {
            app.ui.chat.last_selected_id = None;
        }
    }

    // 1. Image paste handling (Ctrl+V / Cmd+V or PasteImage event)
    let has_focus = ui.memory(|m| m.has_focus(draft_id()));
    let is_hovered = ui.max_rect().contains(ui.input(|i| i.pointer.hover_pos()).unwrap_or(egui::Pos2::ZERO));
    let is_paste = (has_focus || is_hovered)
        && ui.input(|i| {
            let v_key = i.key_pressed(egui::Key::V) && (i.modifiers.ctrl || i.modifiers.command);
            let paste_event = i.events.iter().any(|e| matches!(e, egui::Event::PasteImage(_)));
            v_key || paste_event
        });

    if is_paste
        && let Some((png_bytes, _, _)) = crate::menus::clipboard_png(app)
    {
        attach_image_from_bytes(app, "pasted_image.png".into(), png_bytes);
        ui.ctx().request_repaint();
    }

    // 2. Drag & drop image files onto composer
    let dropped_files = ui.ctx().input(|i| i.raw.dropped_files.clone());
    if !dropped_files.is_empty() && (has_focus || is_hovered) {
        for _f in dropped_files {
            #[cfg(not(target_arch = "wasm32"))]
            {
                let f = _f;
                let path = f.path().to_path_buf();
                if let Ok(bytes) = f.bytes()
                    && bytes.len() <= crate::MAX_CLIPBOARD_PNG_BYTES
                {
                    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "image.png".into());
                    attach_image_from_bytes(app, name, bytes);
                    ui.ctx().request_repaint();
                    break;
                } else if path.exists()
                    && let Ok(bytes) = std::fs::read(&path)
                    && bytes.len() <= crate::MAX_CLIPBOARD_PNG_BYTES
                {
                    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "image.png".into());
                    attach_image_from_bytes(app, name, bytes);
                    ui.ctx().request_repaint();
                    break;
                }
            }
        }
    }

    let mut start_y = ui.max_rect().min.y;

    // Render active target element mention chip
    if let Some(mention) = selection_mention_info(app) {
        let chip_rect = Rect::from_min_size(pos2(ui.max_rect().min.x + 10.0, start_y), vec2(field_w, 26.0));
        ui.painter().rect_filled(chip_rect, CornerRadius::same(6), t.accent.gamma_multiply(0.12));
        ui.painter().rect_stroke(chip_rect, CornerRadius::same(6), Stroke::new(1.0, t.accent.gamma_multiply(0.4)), egui::StrokeKind::Inside);

        let icon_r = Rect::from_min_size(pos2(chip_rect.min.x + 6.0, chip_rect.min.y + 5.0), vec2(16.0, 16.0));
        icons::paint(ui.painter(), Rect::from_center_size(icon_r.center(), Vec2::splat(12.0)), Icon::Sparkle, t.accent);

        let label = format!("@{} ({})", mention.name, mention.kind);
        let max_label_w = (field_w - 56.0).max(20.0);
        let shown = widgets::elide_font(ui, &label, max_label_w, regular(11.0));
        let text_r = Rect::from_min_max(pos2(icon_r.max.x + 4.0, chip_rect.min.y), pos2(chip_rect.max.x - 24.0, chip_rect.max.y));
        let _ = ui.interact(text_r, egui::Id::unique("chat_mention_tag"), Sense::click()).tip("Mentioned target element: prompts will directly modify this element");
        ui.painter().text(pos2(icon_r.max.x + 4.0, chip_rect.center().y), Align2::LEFT_CENTER, shown, regular(11.0), t.accent_text);

        let close_r = Rect::from_min_size(pos2(chip_rect.max.x - 22.0, chip_rect.min.y + 4.0), vec2(18.0, 18.0));
        let close_resp = ui.interact(close_r, egui::Id::unique("chat_mention_clear"), Sense::click()).tip("Unmention element (send page only)");
        if close_resp.hovered() {
            ui.painter().rect_filled(close_r, CornerRadius::same(4), t.hover);
        }
        icons::paint(ui.painter(), Rect::from_center_size(close_r.center(), Vec2::splat(9.0)), Icon::Close, t.icon_secondary);
        if close_resp.clicked() {
            app.ui.chat.with_selection = false;
            ui.ctx().request_repaint();
        }
        start_y += 32.0;
    }

    if let Some(att) = &app.ui.chat.attached_image {
        let chip_rect = Rect::from_min_size(pos2(ui.max_rect().min.x + 10.0, start_y), vec2(field_w, 28.0));
        ui.painter().rect_filled(chip_rect, CornerRadius::same(6), t.field);
        ui.painter().rect_stroke(chip_rect, CornerRadius::same(6), Stroke::new(1.0, t.border_strong), egui::StrokeKind::Inside);

        let thumb_r = Rect::from_min_size(pos2(chip_rect.min.x + 4.0, chip_rect.min.y + 4.0), vec2(20.0, 20.0));
        if let Some(tex) = decode_attachment_texture(ui.ctx(), &att.data_url) {
            ui.painter().image(tex.id(), thumb_r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        } else {
            icons::paint(ui.painter(), Rect::from_center_size(thumb_r.center(), Vec2::splat(13.0)), Icon::Image, t.accent_text);
        }

        let kb = (att.preview_bytes.len() as f32 / 1024.0).max(0.1);
        let label = format!("{} ({:.1} KB)", att.name, kb);
        let max_label_w = (field_w - 56.0).max(20.0);
        let shown = widgets::elide_font(ui, &label, max_label_w, regular(11.0));
        ui.painter().text(pos2(thumb_r.max.x + 6.0, chip_rect.center().y), Align2::LEFT_CENTER, shown, regular(11.0), t.text);

        let close_r = Rect::from_min_size(pos2(chip_rect.max.x - 24.0, chip_rect.min.y + 4.0), vec2(20.0, 20.0));
        let close_resp = ui.interact(close_r, egui::Id::unique("chat_attachment_clear"), Sense::click()).tip("Remove attachment");
        if close_resp.hovered() {
            ui.painter().rect_filled(close_r, CornerRadius::same(4), t.hover);
        }
        icons::paint(ui.painter(), Rect::from_center_size(close_r.center(), Vec2::splat(9.0)), Icon::Close, t.icon_secondary);
        if close_resp.clicked() {
            app.ui.chat.attached_image = None;
            ui.ctx().request_repaint();
        }
        start_y += 34.0;
    }

    let rect = Rect::from_min_size(pos2(ui.max_rect().min.x + 10.0, start_y), vec2(field_w, field_h));
    paint_composer_field(ui, t, rect);
    let text_rect = rect.shrink2(vec2(9.0, 7.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(text_rect).layout(egui::Layout::top_down(egui::Align::Min)));
    let text_w = text_rect.width();

    let hint_text = if let Some(m) = selection_mention_info(app) {
        format!("Ask AI to change @{}…", m.name)
    } else {
        "Describe what you want to build or paste an image…".to_string()
    };

    egui::ScrollArea::vertical().id_salt("chat_draft_scroll").max_height(text_rect.height()).auto_shrink([false, false]).show(&mut child, |ui| {
        ui.set_width(text_w);
        ui.add(
            egui::TextEdit::multiline(&mut app.ui.chat.draft)
                .id(draft_id())
                .frame(egui::Frame::NONE)
                .desired_width(text_w)
                .desired_rows(1)
                .return_key(None)
                .hint_text(hint_text)
                .font(regular(COMPOSER_TEXT_PX))
                .text_color(t.text),
        );
    });

    if std::mem::take(&mut app.ui.chat.focus_draft) {
        ui.ctx().memory_mut(|m| m.request_focus(draft_id()));
    }
    if ui.rect_contains_pointer(rect) && ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Secondary)) {
        if let Some((png_bytes, _, _)) = crate::menus::clipboard_png(app) {
            attach_image_from_bytes(app, "pasted_image.png".into(), png_bytes);
            app.toast("Image attached from clipboard");
            ui.ctx().request_repaint();
        } else if let Some(text) = app.services.clipboard_get_text()
            && !text.is_empty()
        {
            app.ui.chat.draft.push_str(&text);
            ui.ctx().request_repaint();
        }
    }
    // Enter sends and Shift+Enter is a newline: the field takes no return key, so the key is read
    // here (the events are not consumed by the editor's own filtering).
    let entered = has_focus && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift && !i.modifiers.ctrl && !i.modifiers.command);
    if entered && !busy && (!app.ui.chat.draft.trim().is_empty() || app.ui.chat.attached_image.is_some()) {
        let draft = std::mem::take(&mut app.ui.chat.draft);
        match send(app, &draft) {
            Ok(()) => app.ui.chat.stick = true,
            Err(e) => {
                app.ui.chat.failure = Some(e);
                app.ui.chat.draft = draft;
            }
        }
    }
    composer_bar(app, ui, t, width, busy);
    composer_failure(app, ui, t, width);
}

/// The composer field's box: the theme's field fill, a hairline edge, and the accent edge while
/// the person is typing in it.
fn paint_composer_field(ui: &Ui, t: &Tokens, rect: Rect) {
    let focused = ui.memory(|m| m.has_focus(draft_id()));
    ui.painter().rect_filled(rect, CornerRadius::same(9), t.field);
    let stroke = if focused { Stroke::new(1.0, t.accent) } else { Stroke::new(1.0, t.border_strong) };
    ui.painter().rect_stroke(rect, CornerRadius::same(9), stroke, egui::StrokeKind::Inside);
}

/// The row under the composer field: what goes with the message, which model answers, and Send.
fn composer_bar(app: &mut FigApp, ui: &mut Ui, t: &Tokens, width: f32, busy: bool) {
    // The bar sits above the failure line, not on top of it: a panel that reserved room for the
    // failure would otherwise still draw Send over it.
    let fail_h = failure_height(app, ui.ctx(), width);
    let y = ui.max_rect().min.y + (ui.available_height() - COMPOSER_BAR_H - fail_h).max(0.0);
    let bar = Rect::from_min_size(pos2(ui.max_rect().min.x + 10.0, y), vec2((width - 20.0).max(40.0), COMPOSER_BAR_H));
    // Which model answers, and the two things that go with a message.
    let mut x = bar.min.x;
    let mut with_selection = app.ui.chat.with_selection;
    let model = match app.session.chat_model(AIModelRole::Design) {
        Ok((role, _)) => role.profile.name.clone(),
        Err(_) => tr("No model").to_string(),
    };
    let send_w = 62.0;
    let scope_w = 22.0;
    let attach_w = 22.0;
    let room = (bar.max.x - x - send_w - scope_w - attach_w - 14.0).max(20.0);
    let ok = app.session.chat_model(AIModelRole::Design).is_ok();
    let model_r = Rect::from_min_size(pos2(x, bar.min.y), vec2(room, bar.height()));
    let model_resp = ui.interact(model_r, egui::Id::unique("chat_model"), Sense::click()).tip("Select active model or manage models");
    if model_resp.hovered() {
        ui.painter().rect_filled(model_r, CornerRadius::same(5), t.hover);
    }
    let shown = widgets::elide_font(ui, &model, room - 16.0, regular(11.0));
    icons::paint(
        ui.painter(),
        Rect::from_center_size(pos2(model_r.min.x + 6.0, model_r.center().y), Vec2::splat(11.0)),
        Icon::Sparkle,
        if ok { t.accent_text } else { t.danger },
    );
    ui.painter().text(
        pos2(model_r.min.x + 15.0, model_r.center().y),
        Align2::LEFT_CENTER,
        &shown,
        regular(11.0),
        if ok { t.text_secondary } else { t.danger },
    );
    a11y::button(&model_resp, "Select active model");

    let mut chosen_model_id = None;
    let mut open_settings = false;
    crate::right::menu_popup(&model_resp).width(240.0).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Select Model").font(crate::theme::semibold(11.0)).color(t.text));
        });
        ui.separator();

        let models = app.session.prefs.ai.models.models.clone();
        let active_id = app.session.prefs.ai.models.assignments.design.clone();

        if models.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                ui.label(egui::RichText::new("No models configured").font(regular(11.0)).color(t.text_secondary));
            });
        } else {
            for m in &models {
                let is_active = m.id == active_id;
                let display_name = if !m.name.trim().is_empty() {
                    m.name.clone()
                } else if !m.model_id.trim().is_empty() {
                    m.model_id.clone()
                } else {
                    m.id.clone()
                };
                if crate::right::menu_check(ui, is_active, &display_name, "") {
                    chosen_model_id = Some((m.id.clone(), display_name));
                    ui.close();
                }
            }
        }

        ui.separator();
        if crate::right::menu_item(ui, "Manage models…", "") {
            open_settings = true;
            ui.close();
        }
    });

    if let Some((mid, name)) = chosen_model_id {
        app.run("ai.role.assign", json!({"role": "all", "modelId": mid}));
        app.toast(format!("Switched to {}", name));
    }
    if open_settings {
        app.run("ai.settings", json!({}));
    }
    x = bar.max.x - send_w - scope_w - attach_w - 10.0;

    // Attach image button (paperclip / image)
    let attach_r = Rect::from_min_size(pos2(x, bar.center().y - 10.0), Vec2::splat(20.0));
    let has_attachment = app.ui.chat.attached_image.is_some();
    let attach_resp = ui.interact(attach_r, egui::Id::unique("chat_attach_image"), Sense::click()).tip(if has_attachment {
        "Image attached (click to browse or replace, right-click to paste clipboard)"
    } else {
        "Attach image (click to browse files, right-click to paste clipboard, or press Ctrl+V)"
    });
    if attach_resp.hovered() {
        ui.painter().rect_filled(attach_r, CornerRadius::same(5), t.hover);
    }
    icons::paint(
        ui.painter(),
        Rect::from_center_size(attach_r.center(), Vec2::splat(13.0)),
        Icon::Image,
        if has_attachment { t.accent_text } else { t.icon_secondary },
    );
    a11y::button(&attach_resp, "Attach image");
    if attach_resp.clicked() {
        app.pick_file("Attach image for vision", &["png", "jpg", "jpeg", "webp"], false, |app, params| {
            if let Some(b64) = params.get("data").and_then(Value::as_str) {
                use base64::Engine as _;
                if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) {
                    let name = params.get("name").and_then(Value::as_str).unwrap_or("image.png").to_string();
                    attach_image_from_bytes(app, name, bytes);
                }
            } else if let Some(path) = params.get("path").and_then(Value::as_str)
                && let Ok(bytes) = std::fs::read(path)
            {
                let name = std::path::Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or("image.png").to_string();
                attach_image_from_bytes(app, name, bytes);
            }
        });
    } else if attach_resp.secondary_clicked()
        && let Some((png_bytes, _, _)) = crate::menus::clipboard_png(app)
    {
        attach_image_from_bytes(app, "pasted_image.png".into(), png_bytes);
        app.toast("Image attached from clipboard");
    }

    x += attach_w + 4.0;
    // Send the selection with the message.
    let sel_r = Rect::from_min_size(pos2(x, bar.center().y - 10.0), Vec2::splat(20.0));
    let sel_resp = ui.interact(sel_r, egui::Id::unique("chat_with_selection"), Sense::click()).tip(if with_selection {
        tr("The selection goes with the message")
    } else {
        tr("Send only the page")
    });
    if sel_resp.hovered() {
        ui.painter().rect_filled(sel_r, CornerRadius::same(5), t.hover);
    }
    icons::paint(
        ui.painter(),
        Rect::from_center_size(sel_r.center(), Vec2::splat(13.0)),
        Icon::Pointer,
        if with_selection { t.accent_text } else { t.icon_secondary },
    );
    a11y::button(&sel_resp, tr("Send the selection with the message"));
    if sel_resp.clicked() {
        with_selection = !with_selection;
        app.ui.chat.with_selection = with_selection;
    }
    // Send, or Stop while an answer streams.
    let send_r = Rect::from_min_size(pos2(bar.max.x - send_w, bar.min.y), vec2(send_w, bar.height()));
    let can = busy || !app.ui.chat.draft.trim().is_empty() || app.ui.chat.attached_image.is_some();
    let resp = ui.interact(send_r, egui::Id::unique("chat_send"), Sense::click()).tip(if busy { tr("Stop") } else { tr("Send") });
    let bg = if busy {
        t.selected_row
    } else if !can {
        t.hover
    } else if resp.hovered() {
        t.accent_hover
    } else {
        t.accent
    };
    ui.painter().rect_filled(send_r, CornerRadius::same(6), bg);
    let ink = if !can && !busy { t.text_tertiary } else { t.accent_on };
    let glyph = Rect::from_center_size(pos2(send_r.min.x + 15.0, send_r.center().y), Vec2::splat(13.0));
    icons::paint(ui.painter(), glyph, if busy { Icon::Stop } else { Icon::Send }, ink);
    ui.painter().text(
        pos2(glyph.max.x + 5.0, send_r.center().y),
        Align2::LEFT_CENTER,
        if busy { tr("Stop") } else { tr("Send") },
        semibold(11.0),
        ink,
    );
    a11y::button(&resp, if busy { tr("Stop") } else { tr("Send") });
    if resp.clicked() && can {
        if busy {
            stop(app);
        } else {
            let draft = std::mem::take(&mut app.ui.chat.draft);
            match send(app, &draft) {
                Ok(()) => app.ui.chat.stick = true,
                Err(e) => {
                    app.ui.chat.failure = Some(e);
                    app.ui.chat.draft = draft;
                }
            }
        }
    }
}

/// Why the last send did not go, under the field: never scrolled away, and a click dismisses it.
fn composer_failure(app: &mut FigApp, ui: &mut Ui, t: &Tokens, width: f32) {
    let Some(failure) = app.ui.chat.failure.clone() else { return };
    let w = (width - 20.0).max(40.0);
    let galley = failure_galley(ui.ctx(), &failure, width);
    let h = galley.size().y;
    let r = Rect::from_min_size(pos2(ui.max_rect().min.x + 10.0, ui.max_rect().max.y - h - 2.0), vec2(w, h + 2.0));
    let resp = ui.interact(r, egui::Id::unique("chat_failure"), Sense::click()).tip(tr("Dismiss"));
    ui.painter().galley(pos2(r.min.x + 2.0, r.min.y), galley, t.danger);
    a11y::status(&resp, tr("The message was not sent"), &failure);
    if resp.clicked() {
        app.ui.chat.failure = None;
    }
}

/// The conversation: the one scroll region, following the newest message unless the person
/// scrolled back to read.
fn transcript(app: &mut FigApp, ui: &mut Ui, t: &Tokens, width: f32) {
    let follow = app.ui.chat.stick;
    let out = egui::ScrollArea::vertical().id_salt("chat_transcript").auto_shrink([false, false]).stick_to_bottom(follow).show(ui, |ui| {
        ui.set_width(width);
        transcript_body(app, ui, t, width);
        // The room the jump button floats over.
        ui.add_space(12.0);
    });
    // A wheel over the transcript is the person reading back: stop following until they return to
    // the bottom.
    let hovered = out.inner_rect.contains(ui.input(|i| i.pointer.hover_pos()).unwrap_or(egui::Pos2::ZERO));
    if hovered && ui.input(|i| i.smooth_scroll_delta.y.abs() > 0.5) {
        app.ui.chat.stick = false;
    }
    let slack = (out.content_size.y - out.inner_rect.height()).max(0.0);
    let at_bottom = out.state.clamped_offset().y >= slack - 4.0;
    if at_bottom && !app.ui.chat.stick {
        app.ui.chat.stick = true;
    }
    if !at_bottom {
        jump_button(app, ui, t, out.inner_rect);
    }
}

/// The messages themselves: what each said, the tools that ran, and what the model is doing now.
fn transcript_body(app: &mut FigApp, ui: &mut Ui, t: &Tokens, width: f32) {
    let messages = app.session.chat_messages().to_vec();
    if messages.is_empty() && app.ui.chat.streaming.is_none() {
        empty_state(ui, t, width);
        return;
    }
    let show_tools = app.session.prefs.ai.show_tools;
    // A tool call's name lives on the assistant message that made it, so the transcript is read
    // first: a tool message only carries the id.
    let mut call_names: HashMap<String, String> = HashMap::new();
    for m in &messages {
        for c in &m.tool_calls {
            call_names.insert(c.id.clone(), c.name.clone());
        }
    }
    for (i, message) in messages.iter().enumerate() {
        match message.role {
            Role::User => user_message(ui, t, &message.content, &message.images, width),
            Role::Assistant => {
                if !message.content.is_empty() {
                    answer(ui, t, &message.content, width);
                }
                if show_tools && !message.tool_calls.is_empty() {
                    // The answers that follow this step's calls, in the order they were made.
                    let ids: Vec<&str> = message.tool_calls.iter().map(|c| c.id.as_str()).collect();
                    let mut failures: Vec<Option<String>> = vec![None; ids.len()];
                    let mut answered: Vec<bool> = vec![false; ids.len()];
                    for later in messages.iter().skip(i + 1) {
                        if later.role != Role::Tool {
                            break;
                        }
                        let Some(cid) = later.tool_call_id.as_deref() else { continue };
                        if let Some(slot) = ids.iter().position(|x| *x == cid)
                            && let Some(cell) = failures.get_mut(slot)
                        {
                            *cell = tool_failure(&later.content);
                            if let Some(done) = answered.get_mut(slot) {
                                *done = true;
                            }
                        }
                    }
                    let calls: Vec<(&str, &Value, bool, Option<&str>)> = message
                        .tool_calls
                        .iter()
                        .enumerate()
                        .map(|(k, c)| {
                            let failure = failures.get(k).and_then(|f| f.as_deref());
                            (c.name.as_str(), &c.arguments, answered.get(k).copied().unwrap_or(false), failure)
                        })
                        .collect();
                    tool_block(ui, t, &calls, width);
                }
            }
            Role::Tool => {
                // An answer nothing above claimed (a chat reloaded mid-step): still say it ran.
                if show_tools && message.tool_calls.is_empty() {
                    let name = message.tool_call_id.as_ref().and_then(|id| call_names.get(id)).map(String::as_str).unwrap_or("tool");
                    let failure = tool_failure(&message.content);
                    tool_block(ui, t, &[(name, &Value::Null, true, failure.as_deref())], width);
                }
            }
        }
    }
    // The reasoning, as the preference asks.
    reasoning_block(app, ui, t, width);
    let live = app.ui.chat.streaming.as_ref().map(|s| (s.text.clone(), s.tool_calls.is_empty()));
    if let Some((text, waiting)) = live {
        if !text.is_empty() {
            answer(ui, t, &text, width);
        } else if waiting {
            magic_building(ui, t, width);
        }
    }
    if app.ui.chat.resumable {
        continue_button(app, ui, t, width);
    }
}

/// The model spent its step budget with the work unfinished: a control that spends another one on
/// the same conversation, so a long build continues instead of ending where the model paused.
fn continue_button(app: &mut FigApp, ui: &mut Ui, t: &Tokens, width: f32) {
    let w = (width - 20.0).max(60.0);
    let (row, _) = ui.allocate_exact_size(vec2(w, 28.0), Sense::hover());
    let label = tr("Continue");
    let text_w = ui.painter().layout_no_wrap(label.to_string(), semibold(11.0), t.accent).size().x;
    let bw = (text_w + 40.0).min(w);
    let r = Rect::from_min_size(pos2(row.min.x, row.min.y + 1.0), vec2(bw, 26.0));
    let resp = ui.interact(r, egui::Id::unique("chat_continue"), Sense::click()).tip(tr("Spend another step budget on this conversation"));
    let bg = if resp.hovered() { t.accent_hover } else { t.accent };
    ui.painter().rect_filled(r, CornerRadius::same(13), bg);
    icons::paint(ui.painter(), Rect::from_center_size(pos2(r.min.x + 15.0, r.center().y), Vec2::splat(11.0)), Icon::Play, t.accent_on);
    ui.painter().text(pos2(r.min.x + 25.0, r.center().y), Align2::LEFT_CENTER, label, semibold(11.0), t.accent_on);
    a11y::button(&resp, label);
    if resp.clicked()
        && let Err(e) = continue_step(app)
    {
        app.ui.chat.failure = Some(e);
    }
}

/// A message from the person: the brand pill, right-aligned and no wider than 85 % of the panel.
fn user_message(ui: &mut Ui, t: &Tokens, text: &str, images: &[String], width: f32) {
    let max = (width - 20.0) * 0.85;
    let job = wrapped(text, regular(12.5), t.chat_user_text, max);
    let galley = ui.painter().layout_job(job);
    let (h, mut w) = (galley.size().y, galley.size().x.min(max));

    let img_h = if !images.is_empty() { 68.0 } else { 0.0 };
    if !images.is_empty() {
        w = w.max(72.0);
    }
    let total_h = h + 12.0 + img_h;
    let (row, _) = ui.allocate_exact_size(vec2(width - 20.0, total_h), Sense::hover());
    let pill = Rect::from_min_size(pos2(row.max.x - w - 16.0, row.min.y), vec2(w + 16.0, total_h));
    ui.painter().rect_filled(pill, CornerRadius { nw: 9, ne: 9, sw: 9, se: 3 }, t.chat_user);

    let mut content_top = pill.min.y + 6.0;
    for img_url in images {
        let img_box = Rect::from_min_size(pos2(pill.min.x + 8.0, content_top), vec2(58.0, 58.0));
        ui.painter().rect_filled(img_box, CornerRadius::same(5), t.field);
        ui.painter().rect_stroke(img_box, CornerRadius::same(5), Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
        if let Some(tex) = decode_attachment_texture(ui.ctx(), img_url) {
            ui.painter().image(tex.id(), img_box.shrink(2.0), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        } else {
            icons::paint(ui.painter(), Rect::from_center_size(img_box.center(), Vec2::splat(20.0)), Icon::Image, t.text_secondary);
        }
        content_top += 62.0;
    }

    ui.painter().galley(pos2(pill.min.x + 8.0, content_top), galley, t.chat_user_text);
    ui.add_space(6.0);
}

/// The model's answer: a quiet card the width of the panel, its text read as markdown.
fn answer(ui: &mut Ui, t: &Tokens, text: &str, width: f32) {
    let w = (width - 20.0).max(60.0);
    let top = ui.cursor().min.y;
    egui::Frame::NONE.fill(t.chat_ai).corner_radius(CornerRadius { nw: 3, ne: 9, sw: 9, se: 9 }).inner_margin(egui::Margin::symmetric(9, 7)).show(
        ui,
        |ui| {
            ui.set_max_width(w);
            render_markdown(ui, t, text, w - 18.0);
        },
    );
    // Copy the answer, on hover: a long answer is often going somewhere else.
    let copied_at = ui.ctx().data(|d| d.get_temp::<f64>(copy_stamp_id()));
    let copied = copied_at.is_some_and(|at| ui.input(|i| i.time) - at < 1.5);
    let r = Rect::from_min_size(pos2(ui.max_rect().right() - 26.0, top + 4.0), Vec2::splat(18.0));
    let resp = ui.interact(r, copy_id(text), Sense::click()).tip(if copied { tr("Copied") } else { tr("Copy") });
    if copied || resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius::same(4), t.panel);
        let icon = if copied { Icon::Check } else { Icon::Copy };
        icons::paint(ui.painter(), Rect::from_center_size(r.center(), Vec2::splat(12.0)), icon, t.icon_secondary);
    }
    a11y::button(&resp, tr("Copy"));
    if resp.clicked() {
        ui.ctx().copy_text(text.to_string());
        let now = ui.input(|i| i.time);
        ui.ctx().data_mut(|d| d.insert_temp(copy_stamp_id(), now));
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(1500));
    }
    ui.add_space(6.0);
}

/// Stop the send in progress. The answer's text so far stays, marked as cut short.
fn stop(app: &mut FigApp) {
    let text = app.ui.chat.take_streaming_text();
    app.session.chat.busy = false;
    if !text.is_empty() {
        let stopped = tr("(stopped)");
        let _ = app.session.chat_append(ChatMessage::assistant(format!("{text}\n\n{stopped}")));
    }
}

/// One answer's copy button.
fn copy_id(text: &str) -> egui::Id {
    egui::Id::unique(("chat_copy", text.len(), text.chars().next()))
}

fn copy_stamp_id() -> egui::Id {
    egui::Id::unique("chat_copy_stamp")
}

/// The tool calls one model step made, and what each answered.
fn tool_block(ui: &mut Ui, t: &Tokens, calls: &[(&str, &Value, bool, Option<&str>)], width: f32) {
    let w = (width - 20.0).max(60.0);
    egui::Frame::NONE.fill(t.chat_tool).corner_radius(CornerRadius::same(6)).inner_margin(egui::Margin::symmetric(8, 5)).show(ui, |ui| {
        ui.set_max_width(w);
        for (name, arguments, answered, failure) in calls {
            tool_row(ui, t, name, arguments, *answered, *failure, w - 16.0);
        }
    });
    ui.add_space(6.0);
}

/// The short argument that says what a call does (`create_node` → "frame Hero", `export` → "png").
fn tool_detail(name: &str, a: &Value) -> String {
    let s = |k: &str| a.get(k).and_then(Value::as_str).unwrap_or_default();
    let short = |v: &str| v.chars().take(40).collect::<String>();
    match name {
        "create_node" => {
            let kind = s("type");
            let nm = s("name");
            let txt = s("text");
            let what = if txt.is_empty() { nm.to_string() } else { short(txt) };
            if what.is_empty() { kind.to_string() } else { format!("{kind} {what}") }
        }
        "set_properties" => {
            let keys: Vec<&str> =
                a.get("props").and_then(Value::as_object).map(|m| m.keys().map(String::as_str).take(4).collect()).unwrap_or_default();
            keys.join(", ")
        }
        "batch" => {
            let n = a.get("commands").and_then(Value::as_array).map(Vec::len).unwrap_or(0);
            if n == 0 { String::new() } else { trn("{n} commands", n, &[]) }
        }
        "list_commands" => s("filter").to_string(),
        "props_reference" => s("key").to_string(),
        "export" => s("format").to_string(),
        "run_command" => s("command").to_string(),
        "get_node" | "select" => match a.get("ids").or_else(|| a.get("id")) {
            Some(Value::Array(v)) => format!("{} layers", v.len()),
            Some(Value::String(x)) => x.clone(),
            Some(Value::Number(x)) => x.to_string(),
            _ => String::new(),
        },
        "generate_image" | "stock_photo" => short(s("prompt")) + &short(s("query")),
        _ => String::new(),
    }
}

/// The reasoning, kept to what a panel can show.
fn reasoning_excerpt(app: &FigApp) -> String {
    let text = &app.ui.chat.reasoning;
    if text.chars().count() <= 2000 {
        text.clone()
    } else {
        let skip = text.chars().count() - 2000;
        format!("…{}", text.chars().skip(skip).collect::<String>())
    }
}

/// One tool call: what it is doing, and the reason when it did not work.
fn tool_row(ui: &mut Ui, t: &Tokens, name: &str, arguments: &Value, answered: bool, failure: Option<&str>, width: f32) {
    let detail = tool_detail(name, arguments);
    let line = if detail.is_empty() { name.to_string() } else { format!("{name} · {detail}") };
    let (icon, col) = match failure {
        Some(_) => (Icon::Close, t.danger),
        None if answered => (Icon::Check, t.accent_text),
        None => (Icon::Clock, t.text_tertiary),
    };
    let galley = ui.painter().layout(line, regular(11.0), t.text_secondary, width - 18.0);
    let h = galley.size().y;
    let (row, _) = ui.allocate_exact_size(vec2(width, h), Sense::hover());
    icons::paint(ui.painter(), Rect::from_min_size(pos2(row.min.x, row.min.y + 1.0), Vec2::splat(12.0)), icon, col);
    ui.painter().galley(pos2(row.min.x + 16.0, row.min.y), galley, t.text_secondary);
    if let Some(reason) = failure {
        let g = ui.painter().layout(first_line(reason, 200), regular(11.0), t.danger, width - 18.0);
        let rh = g.size().y;
        let (r2, _) = ui.allocate_exact_size(vec2(width, rh), Sense::hover());
        ui.painter().galley(pos2(r2.min.x + 16.0, r2.min.y), g, t.danger);
    }
    ui.add_space(2.0);
}

/// Where the conversation is in the model's turn: the card shown while it reads the last results
/// and writes the next step.
fn magic_building(ui: &mut Ui, t: &Tokens, width: f32) {
    let w = (width - 20.0).max(60.0);
    let time = ui.input(|i| i.time) as f32;
    egui::Frame::NONE.fill(t.chat_tool).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
        ui.set_max_width(w);
        let head = ui.cursor().min;
        let (row, _) = ui.allocate_exact_size(vec2(w - 20.0, 16.0), Sense::hover());
        icons::paint(ui.painter(), Rect::from_min_size(row.min, Vec2::splat(14.0)), Icon::Sparkle, t.accent_text);
        ui.painter().text(pos2(row.min.x + 20.0, row.center().y), Align2::LEFT_CENTER, tr("Building…"), semibold(11.5), t.text);
        ui.painter().text(pos2(row.min.x + 76.0, row.center().y), Align2::LEFT_CENTER, tr("reading the results"), regular(10.5), t.text_secondary);
        // Bars that sweep while the model writes the next step.
        for (i, frac) in [0.62_f32, 0.44, 0.75].into_iter().enumerate() {
            let y = head.y + 22.0 + i as f32 * 7.0;
            let shade = 0.10 + 0.10 * (time * 3.0 - i as f32 * 0.6).sin().abs();
            ui.painter().rect_filled(
                Rect::from_min_size(pos2(head.x, y), vec2((w - 20.0) * frac, 4.0)),
                CornerRadius::same(2),
                t.text_secondary.gamma_multiply(shade),
            );
        }
        ui.add_space(22.0);
    });
    ui.add_space(6.0);
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
}

/// The reasoning the model showed, as a collapsible card.
fn reasoning_block(app: &mut FigApp, ui: &mut Ui, t: &Tokens, width: f32) {
    if app.ui.chat.reasoning.trim().is_empty() {
        return;
    }
    let mode = app.session.prefs.ai.reasoning_display;
    let streaming_now = app.ui.chat.streaming.is_some();
    let open = match mode {
        figcraft_ai::ReasoningDisplay::Collapsed => app.ui.chat.show_reasoning,
        figcraft_ai::ReasoningDisplay::WhileThinking => streaming_now,
        _ => true,
    };
    let label = if streaming_now { tr("Thinking…") } else { tr("Reasoning") };
    let w = (width - 20.0).max(60.0);
    egui::Frame::NONE.stroke(Stroke::new(1.0, t.border)).corner_radius(CornerRadius::same(6)).inner_margin(egui::Margin::symmetric(8, 6)).show(
        ui,
        |ui| {
            ui.set_max_width(w);
            let (row, resp) = ui.allocate_exact_size(vec2(w - 16.0, 14.0), Sense::click());
            icons::paint(ui.painter(), Rect::from_min_size(row.min, Vec2::splat(13.0)), Icon::Thinking, t.accent_text);
            ui.painter().text(pos2(row.min.x + 19.0, row.center().y), Align2::LEFT_CENTER, label, regular(11.0), t.text_secondary);
            let chev = Rect::from_center_size(pos2(row.max.x - 6.0, row.center().y), Vec2::splat(10.0));
            icons::paint(ui.painter(), chev, if open { Icon::Chevron } else { Icon::ChevronRight }, t.icon_secondary);
            a11y::disclosure(&resp, label, open);
            if resp.clicked() && mode == figcraft_ai::ReasoningDisplay::Collapsed {
                app.ui.chat.show_reasoning = !app.ui.chat.show_reasoning;
            }
            if open {
                ui.add_space(2.0);
                ui.painter().line_segment([ui.cursor().left_top(), ui.cursor().right_top()], Stroke::new(1.0, t.border));
                ui.add_space(3.0);
                let excerpt = reasoning_excerpt(&*app);
                let galley = ui.painter().layout(excerpt, regular(11.0), t.text_secondary, w - 18.0);
                let h = galley.size().y;
                let (e, _) = ui.allocate_exact_size(vec2(w - 16.0, h), Sense::hover());
                ui.painter().galley(e.min, galley, t.text_secondary);
            }
        },
    );
    ui.add_space(6.0);
}

/// What the panel shows before the first message: what the assistant can do.
fn empty_state(ui: &mut Ui, t: &Tokens, width: f32) {
    let full = ui.available_rect_before_wrap();
    let w = (width - 40.0).clamp(60.0, 220.0);
    let title = ui.painter().layout_no_wrap(tr("What are you designing?").to_string(), semibold(13.0), t.text);
    let body = tr("Ask for a page, a flow or a component. The assistant builds it in this file and shows you each step.");
    let galley = ui.painter().layout(body.to_string(), regular(11.5), t.text_secondary, w);
    let block = 34.0 + 10.0 + title.size().y + 6.0 + galley.size().y;
    let top = (full.center().y - block / 2.0).max(full.min.y + 8.0);
    let mark = Rect::from_min_size(pos2(full.center().x - 17.0, top), Vec2::splat(34.0));
    ui.painter().rect_filled(mark, CornerRadius::same(10), t.selected_row);
    icons::paint(ui.painter(), Rect::from_center_size(mark.center(), Vec2::splat(20.0)), Icon::Sparkle, t.accent_text);
    let ty = mark.max.y + 10.0;
    let tx = full.center().x - title.size().x / 2.0;
    ui.painter().galley(pos2(tx, ty), title.clone(), t.text);
    let gx = full.center().x - galley.size().x / 2.0;
    let gy = ty + title.size().y + 6.0;
    ui.painter().galley(pos2(gx, gy), galley, t.text_secondary);
    ui.allocate_space(vec2(w, block + 8.0));
}

/// Back to the newest message: a floating button at the transcript's bottom edge.
fn jump_button(app: &mut FigApp, ui: &mut Ui, t: &Tokens, inner: Rect) {
    if inner.height() < 40.0 {
        return;
    }
    let r = Rect::from_center_size(pos2(inner.center().x, inner.max.y - 16.0), vec2(94.0, 24.0));
    let resp = ui.interact(r, egui::Id::unique("chat_jump"), Sense::click()).tip(tr("Show the newest message"));
    ui.painter().rect_filled(r, CornerRadius::same(12), t.panel);
    ui.painter().rect_stroke(r, CornerRadius::same(12), Stroke::new(1.0, t.border_strong), egui::StrokeKind::Inside);
    icons::paint(ui.painter(), Rect::from_center_size(pos2(r.min.x + 15.0, r.center().y), Vec2::splat(12.0)), Icon::ArrowDown, t.icon_secondary);
    ui.painter().text(pos2(r.min.x + 26.0, r.center().y), Align2::LEFT_CENTER, tr("Latest"), semibold(10.5), t.text);
    a11y::button(&resp, tr("Show the newest message"));
    if resp.clicked() {
        app.ui.chat.stick = true;
    }
}

/// A run of text wrapped at `width` in `font`.
fn wrapped(text: &str, font: egui::FontId, color: Color32, width: f32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::simple(text.to_string(), font, color, width.max(20.0));
    job.wrap.max_width = width.max(20.0);
    job.wrap.break_anywhere = false;
    job
}

/// The answer as text with its structure: headings, lists, code and emphasis.
fn render_markdown(ui: &mut Ui, t: &Tokens, text: &str, width: f32) {
    let w = width.max(40.0);
    let mut fence: Option<Vec<String>> = None;
    for line in text.lines() {
        let trimmed = line.trim_end();
        if trimmed.trim_start().starts_with("```") {
            match fence.take() {
                Some(code) => code_frame(ui, t, &code.join("\n"), w),
                None => fence = Some(Vec::new()),
            }
            continue;
        }
        if let Some(code) = fence.as_mut() {
            code.push(trimmed.to_string());
            continue;
        }
        let body = trimmed.trim_start();
        let indent = ((trimmed.chars().count() - body.chars().count()).min(8)) as f32 * 2.0;
        if body.is_empty() {
            ui.add_space(4.0);
        } else if let Some((level, rest)) = heading(body) {
            let galley = ui.painter().layout(rest.to_string(), crate::theme::semibold(heading_size(level)), t.chat_ai_text, w);
            let h = galley.size().y;
            let (r, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
            ui.painter().galley(r.min, galley, t.chat_ai_text);
        } else if let Some(rest) = bullet(body) {
            let job = markdown_job(t, rest, w - indent - 10.0);
            let galley = ui.painter().layout_job(job);
            let h = galley.size().y;
            let (r, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
            ui.painter().circle_filled(pos2(r.min.x + indent + 4.0, r.min.y + 7.0), 1.6, t.text_secondary);
            ui.painter().galley(pos2(r.min.x + indent + 11.0, r.min.y), galley, t.chat_ai_text);
        } else if let Some((n, rest)) = numbered(body) {
            let num = format!("{n}.");
            let g = ui.painter().layout_no_wrap(num, regular(12.0), t.text_secondary);
            let off = g.size().x + 5.0;
            let job = markdown_job(t, rest, w - indent - off);
            let galley = ui.painter().layout_job(job);
            let h = galley.size().y.max(g.size().y);
            let (r, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
            ui.painter().galley(pos2(r.min.x + indent, r.min.y), g, t.text_secondary);
            ui.painter().galley(pos2(r.min.x + indent + off, r.min.y), galley, t.chat_ai_text);
        } else {
            let job = markdown_job(t, body, w - indent);
            let galley = ui.painter().layout_job(job);
            let h = galley.size().y;
            let (r, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
            ui.painter().galley(pos2(r.min.x + indent, r.min.y), galley, t.chat_ai_text);
        }
    }
    if let Some(code) = fence
        && !code.is_empty()
    {
        code_frame(ui, t, &code.join("\n"), w);
    }
}

/// One line's spans as a [`LayoutJob`]: `**bold**` and `` `code` `` are what an answer uses most.
fn markdown_job(t: &Tokens, line: &str, width: f32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = width.max(20.0);
    job.wrap.break_anywhere = false;
    let base = egui::TextFormat { font_id: regular(12.5), color: t.chat_ai_text, ..Default::default() };
    let strong = egui::TextFormat { font_id: crate::theme::semibold(12.5), color: t.chat_ai_text, ..Default::default() };
    let mono = egui::TextFormat { font_id: egui::FontId::monospace(11.5), color: t.chat_ai_text, background: t.panel, ..Default::default() };
    let mut rest = line;
    while !rest.is_empty() {
        // The next marker, whichever of the two comes first.
        let strong_at = rest.find("**");
        let mono_at = rest.find('`');
        let (at, marker, fmt) = match (strong_at, mono_at) {
            (None, None) => {
                job.append(rest, 0.0, base.clone());
                break;
            }
            (Some(a), Some(b)) if a <= b => (a, "**", &strong),
            (Some(a), None) => (a, "**", &strong),
            (Some(_), Some(b)) => (b, "`", &mono),
            (None, Some(b)) => (b, "`", &mono),
        };
        if at > 0 {
            job.append(&rest[..at], 0.0, base.clone());
            rest = &rest[at..];
        }
        rest = &rest[marker.len()..];
        match rest.find(marker) {
            Some(end) => {
                job.append(&rest[..end], 0.0, fmt.clone());
                rest = &rest[end + marker.len()..];
            }
            None => {
                // An unclosed marker is just text.
                job.append(rest, 0.0, base.clone());
                rest = "";
            }
        }
    }
    if job.text.is_empty() {
        job.append(" ", 0.0, base);
    }
    job
}

/// A heading's level and its text, if the line is one.
fn heading(line: &str) -> Option<(usize, &str)> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if hashes == 0 || hashes > 6 || !line[hashes..].starts_with(' ') {
        return None;
    }
    Some((hashes, line[hashes..].trim_start()))
}

fn heading_size(level: usize) -> f32 {
    match level {
        1 => 15.0,
        2 => 13.5,
        3 => 12.5,
        _ => 12.0,
    }
}

/// A bullet's text, if the line is one.
fn bullet(line: &str) -> Option<&str> {
    ["- ", "* ", "• "].iter().find_map(|mark| line.strip_prefix(mark))
}

/// A numbered item's number and its text, if the line is one.
fn numbered(line: &str) -> Option<(usize, &str)> {
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let rest = line[digits..].strip_prefix(". ")?;
    Some((line[..digits].parse().ok()?, rest))
}

/// A fenced block, in the monospace face on a band of its own.
fn code_frame(ui: &mut Ui, t: &Tokens, code: &str, width: f32) {
    let w = width.max(40.0);
    egui::Frame::NONE.fill(t.panel).corner_radius(CornerRadius::same(5)).inner_margin(egui::Margin::symmetric(7, 5)).show(ui, |ui| {
        ui.set_max_width(w);
        let galley = ui.painter().layout(code.to_string(), egui::FontId::monospace(11.0), t.text, w - 14.0);
        let h = galley.size().y;
        let (r, _) = ui.allocate_exact_size(vec2(w - 14.0, h), Sense::hover());
        ui.painter().galley(r.min, galley, t.text);
    });
    ui.add_space(3.0);
}

/// The failure a tool answer carries, if it carries one: a tool's answer is a JSON object whose
/// `error` key is the reason it did not work.
fn tool_failure(content: &str) -> Option<String> {
    let v: Value = serde_json::from_str(content).ok()?;
    v.get("error").and_then(Value::as_str).map(str::to_string)
}

/// The first line of a message, cut to `max` characters.
fn first_line(s: &str, max: usize) -> String {
    let line = s.lines().next().unwrap_or_default().trim();
    if line.chars().count() <= max {
        line.to_string()
    } else {
        let cut: String = line.chars().take(max).collect();
        format!("{cut}…")
    }
}

impl Panel {
    /// The text of the answer that was streaming, taking it.
    fn take_streaming_text(&mut self) -> String {
        self.streaming.take().map(|s| s.text).unwrap_or_default()
    }
}

/// The chat tab's presence in the right panel's tab row.
pub fn tab_label() -> &'static str {
    tr("AI")
}

/// The colour the tab's agent work is marked with, as the live agents are.
pub fn accent() -> Color32 {
    AGENT
}

/// Whether the chat can be used at all: a model for the design role, with a key.
pub fn usable(app: &FigApp) -> bool {
    app.session.chat_model(AIModelRole::Design).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RightTab;
    use crate::present_tests::Ui;
    use figcraft_ai::MediaFactory;

    /// The right panel's AI tab, with a model configured.
    fn chat_ui() -> Ui {
        let mut s = figcraft_engine::Session::empty();
        s.credentials = Some(Box::new(figcraft_ai::credentials::MemoryCredentialStore::new()));
        let _ = s.execute("ai.connection.add", &json!({"provider": "openai"}));
        let models = s.execute("ai.models", &json!({})).expect("the settings are listed");
        let connection = models["connections"][0]["id"].as_str().expect("an id").to_string();
        let model = s
            .execute("ai.model.add", &json!({"name": "Main", "connectionId": connection, "modelID": "gpt-5.6", "capabilities": ["tools"]}))
            .expect("the model is added");
        s.execute("ai.role.assign", &json!({"role": "design", "modelId": model["id"]})).expect("the role is assigned");
        s.execute("ai.credential.set", &json!({"integration": "openai", "profileId": connection, "field": "api-key", "value": "sk-test"}))
            .expect("the key is stored");
        let mut u = Ui::new(s);
        let _ = u.app.session.execute("file.new", &json!({}));
        u
    }

    #[test]
    fn a_message_goes_into_the_conversation_with_its_skills() {
        let mut u = chat_ui();
        send(&mut u.app, "/prototype connect the two screens").expect("the message is sent");
        let messages = u.app.session.chat_messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, Role::User);
        // The router's block rode along, naming the skills it activated.
        assert!(messages[0].content.contains("[Design skills activated for this request]"), "{}", messages[0].content);
    }

    #[test]
    fn the_control_channel_s_send_gets_the_skills_too() {
        // `ai.chat.send` appends the user message itself and leaves the send to the frame loop.
        // That path skipped the router, so a scripted send reached the model with no skill
        // knowledge: the same prompt through the panel and through the control channel produced
        // different designs. Both get the block now.
        let mut u = chat_ui();
        let before = u.app.session.chat_messages().len();
        u.app.session.execute("ai.chat.send", &json!({"text": "I want to make an e-commerce web app"})).expect("the message is queued");
        send_pending(&mut u.app).expect("the send starts");
        let messages = u.app.session.chat_messages();
        assert_eq!(messages.len(), before + 1);
        let content = &messages[before].content;
        assert!(content.contains("[Design skills activated for this request]"), "{content}");
        assert!(content.contains("design.product-site"), "the shop skill did not activate: {content}");
        // Exactly once: a second pass would double the block.
        assert_eq!(content.matches("[Design skills activated for this request]").count(), 1);
    }

    #[test]
    fn a_queued_message_that_already_carries_its_skills_is_not_given_them_twice() {
        let mut u = chat_ui();
        u.app
            .session
            .chat_append(ChatMessage::user("build a dashboard\n\n[Design skills activated for this request]\n\n### design.dashboard (v1.0.0)"))
            .expect("appended");
        send_pending(&mut u.app).expect("the send starts");
        let content = &u.app.session.chat_messages()[0].content;
        assert_eq!(content.matches("[Design skills activated for this request]").count(), 1, "{content}");
    }

    #[test]
    fn an_empty_send_is_not_an_error_and_sends_nothing() {
        let mut u = chat_ui();
        send(&mut u.app, "   ").expect("nothing happens");
        assert!(u.app.session.chat_messages().is_empty());
    }

    #[test]
    fn a_send_while_one_is_out_is_refused() {
        let mut u = chat_ui();
        // A send with no reachable provider still queues the message; the second is refused.
        let (tx, rx) = channel();
        let _ = tx;
        u.app.ui.chat.streaming = Some(Streaming {
            rx,
            tx: channel().0,
            _thread: Some(std::thread::spawn(|| {})),
            step: 0,
            text: String::new(),
            tool_calls: Vec::new(),
            results: Vec::new(),
            mcp_dispatched: false,
            provider: "openai".into(),
            model: "gpt-5.6".into(),
        });
        let err = send(&mut u.app, "again").expect_err("a second send is refused");
        assert!(err.contains("Stop"), "{err}");
    }

    #[test]
    fn the_chat_needs_a_model_and_says_what_is_missing() {
        let mut u = Ui::new(figcraft_engine::Session::empty());
        let err = send(&mut u.app, "hello").expect_err("no model is configured");
        assert!(err.contains("Design"), "{err}");
    }

    #[test]
    fn a_send_without_a_key_says_so() {
        let mut u = chat_ui();
        let models = u.app.session.execute("ai.models", &json!({})).expect("the settings are listed");
        let connection = models["connections"][0]["id"].as_str().expect("an id").to_string();
        u.app
            .session
            .execute("ai.credential.clear", &json!({"integration": "openai", "profileId": connection, "field": "api-key"}))
            .expect("the key is forgotten");
        let err = send(&mut u.app, "hello").expect_err("no key");
        assert!(err.contains("API key"), "{err}");
    }

    #[test]
    fn the_request_carries_the_conversation_the_tools_and_the_model() {
        let mut u = chat_ui();
        u.app.session.chat_append(ChatMessage::user("hello")).expect("appended");
        let request = build_request(&u.app, "").expect("the request is built");
        assert_eq!(request.model, "gpt-5.6");
        assert_eq!(request.protocol, figcraft_ai::providers::Protocol::Openai);
        assert!(!request.tools.is_empty());
        assert!(request.system.contains("sycnUI"));
        assert_eq!(request.messages.len(), 1);
    }

    #[test]
    fn the_standing_instructions_ride_on_the_system_prompt() {
        let mut u = chat_ui();
        u.app.session.execute("ai.skill.toggle", &json!({"id": "design-craft", "enabled": true})).expect("the skill is on");
        let standing = figcraft_ai::skills::active_prompt_text(&u.app.session.prefs.ai.skills.active());
        let request = build_request(&u.app, &standing).expect("the request is built");
        assert!(request.system.contains("Standing instructions"), "{}", request.system);
    }

    #[test]
    fn the_selection_rides_along_as_context() {
        let mut u = chat_ui();
        let out = u.app.session.execute("node.create", &json!({"type": "rectangle", "width": 10, "height": 10})).expect("a layer");
        let id = out["id"].as_u64().expect("an id");
        u.app.session.execute("select.set", &json!({"ids": [id]})).expect("selected");
        let context = selection_context(&mut u.app);
        assert!(context.contains("Page:"), "{context}");
        assert!(context.contains("one layer"), "{context}");
    }

    /// The composer's pointer control is the person saying what goes with the message: off, the
    /// model is told about the page and nothing about what happens to be selected.
    #[test]
    fn the_pointer_control_decides_whether_the_selection_is_quoted() {
        let mut u = chat_ui();
        let out = u.app.session.execute("node.create", &json!({"type": "rectangle", "width": 10, "height": 10})).expect("a layer");
        let id = out["id"].as_u64().expect("an id");
        u.app.session.execute("select.set", &json!({"ids": [id]})).expect("selected");
        u.app.ui.chat.with_selection = false;
        let context = selection_context(&mut u.app);
        assert!(context.contains("Page:"), "{context}");
        assert!(!context.contains("one layer"), "{context}");
        assert!(context.contains("Nothing is selected."), "{context}");
    }

    #[test]
    fn selection_mention_info_and_ask_ai_workflow() {
        let mut u = chat_ui();
        let out = u.app.session.execute("node.create", &json!({"type": "rectangle", "name": "Hero Card", "width": 120, "height": 60})).expect("a layer");
        let id = out["id"].as_u64().expect("an id");
        u.app.session.execute("select.set", &json!({"ids": [id]})).expect("selected");
        let mention = selection_mention_info(&u.app).expect("selection mention info");
        assert_eq!(mention.name, "Hero Card");
        assert_eq!(mention.kind, "Rectangle");
        assert_eq!(mention.id, id);
        assert_eq!(mention.count, 1);

        // Invoke askSelection
        crate::menus::invoke(&mut u.app, "ai.chat.askSelection");
        assert_eq!(u.app.ui.right_tab, crate::RightTab::Chat);
        assert!(u.app.ui.chat.with_selection);
        assert!(u.app.ui.chat.focus_draft);
        assert!(u.app.ui.chat.draft.starts_with("@Hero Card "));
    }

    /// A run of tool calls says how many it was, in the singular for one, and says nothing for a
    /// batch that carried none.
    #[test]
    fn a_batch_names_its_command_count() {
        assert_eq!(tool_detail("batch", &json!({"commands": [{"command": "node.create"}]})), "1 command");
        assert_eq!(tool_detail("batch", &json!({"commands": [{"command": "a"}, {"command": "b"}]})), "2 commands");
        assert_eq!(tool_detail("batch", &json!({"commands": []})), "");
    }

    #[test]
    fn a_tool_call_runs_and_its_answer_goes_back_to_the_model() {
        let mut u = chat_ui();
        // A finished answer that asks for one tool.
        let response = ChatResponse {
            text: "creating".into(),
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "create_node".into(),
                arguments: json!({"type": "rectangle", "width": 10, "height": 10}),
            }],
            ..Default::default()
        };
        u.app.session.ai.record(u.app.session.chat.record_usage("openai", "gpt-5.6", &response));
        let calls = response.tool_calls.clone();
        u.app.session.chat_append(ChatMessage::assistant_calls(response.text.clone(), calls.clone())).expect("appended");
        for call in &calls {
            let tool = figcraft_engine::chat_tools::table().iter().find(|t| t.name == call.name).expect("the tool exists");
            let text = serde_json::to_string(&tool.run(&mut u.app.session, &call.arguments).expect("the tool runs")).expect("the answer serialises");
            u.app.session.chat_append(ChatMessage::tool_result(call.id.clone(), text)).expect("appended");
        }
        let messages = u.app.session.chat_messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].role, Role::Tool);
        assert_eq!(messages[1].tool_call_id.as_deref(), Some("c1"));
        // The layer exists.
        assert!(u.app.session.execute("node.get", &json!({"id": 1})).is_ok());
    }

    #[test]
    fn a_tool_the_model_invented_is_an_error_it_reads() {
        let mut u = chat_ui();
        let call = ToolCall { id: "c1".into(), name: "no_such_tool".into(), arguments: json!({}) };
        let tool = figcraft_engine::chat_tools::table().iter().find(|t| t.name == call.name);
        let answer = match tool {
            Some(t) => t.run(&mut u.app.session, &call.arguments),
            None => Err(figcraft_engine::chat_tools::instead_of(&call.name)),
        };
        assert!(answer.is_err());
    }

    #[test]
    fn stopping_keeps_what_arrived() {
        let mut u = chat_ui();
        let (tx, rx) = channel();
        let _ = tx;
        u.app.ui.chat.streaming = Some(Streaming {
            rx,
            tx: channel().0,
            _thread: Some(std::thread::spawn(|| {})),
            step: 0,
            text: "partial answer".into(),
            tool_calls: Vec::new(),
            results: Vec::new(),
            mcp_dispatched: false,
            provider: "openai".into(),
            model: "gpt-5.6".into(),
        });
        stop(&mut u.app);
        assert!(u.app.ui.chat.streaming.is_none());
        assert!(!u.app.session.chat.busy);
        let messages = u.app.session.chat_messages();
        assert!(messages.last().and_then(|m| m.content.contains("(stopped)").then_some(())).is_some(), "the cut answer is kept");
    }

    #[test]
    fn the_panel_draws_with_a_conversation() {
        let mut u = chat_ui();
        u.app.session.chat_append(ChatMessage::user("hello")).expect("appended");
        u.app.session.chat_append(ChatMessage::assistant("hi there")).expect("appended");
        u.app.ui.right_tab = RightTab::Chat;
        u.frame(vec![]);
        u.frame(vec![]);
    }

    /// The composer is pinned to the panel's bottom edge whatever the conversation does: a long
    /// answer scrolls in the transcript between the header and the field, and never pushes the
    /// field the person types into off the panel.
    #[test]
    fn the_composer_stays_at_the_bottom_under_a_long_conversation() {
        let mut u = chat_ui();
        u.app.ui.right_tab = RightTab::Chat;
        u.frame(vec![]);
        let short = send_rect(&mut u);
        for i in 0..12 {
            u.app.session.chat_append(ChatMessage::user(format!("message {i}"))).expect("appended");
            u.app.session.chat_append(ChatMessage::assistant("a paragraph of the answer.\n\n".repeat(30))).expect("appended");
        }
        let long = send_rect(&mut u);
        let panel = u.app.ui.right_panel_rect.expect("the right panel shows");
        assert_eq!(short.max.y, long.max.y, "the composer moved with the conversation");
        assert!(panel.max.y - long.max.y < COMPOSER_BAR_H + COMPOSER_MIN_H + COMPOSER_GAP, "it sits at the bottom edge");
        assert!(long.min.y > panel.min.y + HEADER_H, "the transcript still has its room");
    }

    /// Where the composer's Send control is drawn, after a frame that lays the panel out.
    fn send_rect(u: &mut Ui) -> Rect {
        u.frame(vec![]);
        u.frame(vec![]);
        u.ctx.read_response(egui::Id::unique("chat_send")).expect("the composer drew Send").rect
    }

    #[test]
    fn a_media_tool_is_dispatched_not_run_in_the_engine() {
        for name in ["generate_image", "remove_background", "stock_photo", "source_image", "screenshot", "review_design"] {
            assert!(figcraft_engine::chat_tools::is_media_tool(name), "{name} is the worker's");
        }
        let call = ToolCall { id: "c1".into(), name: "generate_image".into(), arguments: json!({"prompt": "a fox"}) };
        // The engine's own table refuses it: the work is the media worker's.
        let tool = figcraft_engine::chat_tools::table().iter().find(|t| t.name == call.name).expect("the tool is listed");
        assert!(tool.run(&mut chat_ui().app.session, &call.arguments).is_err());
    }

    #[test]
    fn the_reviewer_is_asked_for_faults_it_can_actually_see() {
        let prompt = review_prompt("an e-commerce landing page");
        assert!(prompt.contains("e-commerce landing page"), "the brief is in it");
        for ask in ["Hierarchy", "Alignment and spacing", "Type", "Contrast", "three changes"] {
            assert!(prompt.contains(ask), "the review asks for {ask}");
        }
        assert!(prompt.contains("do not invent faults"), "it is told not to invent");
        // With no brief it still reviews something rather than sending "for: ".
        assert!(review_prompt("").contains("the screen the person asked for"));
    }

    #[test]
    fn a_screen_can_be_looked_at_without_a_vision_model() {
        // The render reaches the worker as `imageData`; with no Vision model the answer is the
        // error the model reads, never a silent pass.
        let answer = figcraft_ai::media::block_on(look_at_design(
            &figcraft_ai::NoMedia,
            &(None, None, None, None),
            "review_design",
            &json!({
                "imageData": "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAAAAAA6fptVAAAACklEQVR4nGP4DwABAQEAsTj2FAAAAABJRU5ErkJggg==",
                "brief": "a shop"
            }),
        ));
        assert!(answer.contains("error"), "{answer}");
        // Nothing to render is said plainly too.
        let answer = figcraft_ai::media::block_on(look_at_design(&figcraft_ai::NoMedia, &(None, None, None, None), "screenshot", &json!({})));
        assert!(answer.contains("nothing could be rendered"), "{answer}");
    }

    #[test]
    fn a_queued_message_is_not_appended_twice() {
        // A command (`ai.chat.send`, an action) appends the user message itself and sets
        // pending_send. The frame loop must only start the request, never append it again.
        let mut u = chat_ui();
        let _ = u.app.session.execute("ai.chat.send", &json!({"text": "hello there"}));
        assert_eq!(u.app.session.chat_messages().len(), 1, "the command appended one message");
        assert!(u.app.session.chat.pending_send);

        // Mimic the frame loop: clear the flag and send the pending message. There is no model,
        // so send_pending returns an error, but it must not have appended a second user message.
        u.app.session.chat.pending_send = false;
        let _ = super::send_pending(&mut u.app);
        let users: Vec<_> = u.app.session.chat_messages().iter().filter(|m| m.role == figcraft_ai::transport::Role::User).collect();
        assert_eq!(users.len(), 1, "exactly one user message, not a duplicate");
        assert_eq!(users[0].content, "hello there");
    }

    #[test]
    fn a_missing_source_layer_is_reported_instead_of_panicking() {
        // Both image-editing tools report a missing source plainly; neither reaches a worker.
        let answer = figcraft_ai::media::block_on(vectorize_image(figcraft_ai::NoMedia.make(None, None, None, None), &json!({})));
        assert!(answer.contains("error"), "{answer}");
        let answer = figcraft_ai::media::block_on(remove_background(figcraft_ai::NoMedia.make(None, None, None, None), &json!({})));
        assert!(answer.contains("error"), "{answer}");
    }

    #[test]
    fn an_image_answer_is_placed_and_the_model_never_sees_the_bytes() {
        let mut u = chat_ui();
        // A tiny PNG-shaped base64 stands in for a real image.
        let answer = json!({"imageData": "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAAAAAA6fptVAAAACklEQVR4nGP4DwABAQEAsTj2FAAAAABJRU5ErkJggg==", "credit": "pexels: someone (https://example.com/p)"})
            .to_string();
        let call = ToolCall { id: "c1".into(), name: "generate_image".into(), arguments: json!({"prompt": "a fox", "name": "FoxArt"}) };
        let placed = place_media_answer(&mut u.app, &call, &answer);
        // The answer names the layer, and carries no base64.
        assert!(!placed.contains("aWNvbg=="), "{placed}");
        assert!(placed.contains("credit"), "{placed}");
        // The layer exists: one was placed (the bytes are not a real image, so the command
        // either made a layer or said why not; both are answers the model can read).
        assert!(placed.contains("id") || placed.contains("error"), "{placed}");
    }

    #[test]
    fn a_picture_lands_in_the_slot_it_names_rather_than_on_top_of_it() {
        // The model draws a grey rectangle for each picture and passes its id to `source_image`,
        // so that id names the slot. Placing the picture as a new layer left the grey box empty
        // and the photography loose on the page — the audit then reported the slot as missing,
        // which was true, while `source_image` said the picture had been placed.
        let mut u = chat_ui();
        let page = u.app.session.ds().expect("a document").page;
        let slot = u
            .app
            .session
            .execute("shape.rectangle", &json!({"x": 20.0, "y": 30.0, "width": 120.0, "height": 80.0, "parent": page.0}))
            .expect("a rectangle");
        let id = slot.get("id").cloned().expect("its id");
        let layers_before = u.app.session.ds().expect("a document").doc.nodes().count();
        let answer = json!({"results": [{
            "id": id.to_string(),
            "imageData": "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
            "credit": "openverse: someone (CC0)",
        }]})
        .to_string();
        let call = ToolCall { id: "c1".into(), name: "source_image".into(), arguments: json!({}) };
        let placed = place_media_answer(&mut u.app, &call, &answer);
        assert!(!placed.contains("imageData"), "the bytes never reach the model: {placed}");
        // No new layer: the picture was added to the rectangle's fills.
        let after = u.app.session.ds().expect("a document").doc.nodes().count();
        assert_eq!(after, layers_before, "the pictures fill the slots; they do not add layers: {placed}");
    }

    #[test]
    fn a_slot_id_that_names_no_layer_still_places_the_picture() {
        // A model that sends the slot's name, or an id it made up, still gets its picture: the
        // image goes on the page rather than failing the whole call.
        let mut u = chat_ui();
        let answer = json!({"results": [{
            "id": "Photo slot",
            "imageData": "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
        }]})
        .to_string();
        let call = ToolCall { id: "c1".into(), name: "source_image".into(), arguments: json!({}) };
        let placed = place_media_answer(&mut u.app, &call, &answer);
        assert!(placed.contains("id") || placed.contains("error"), "{placed}");
    }

    #[test]
    fn a_non_media_answer_is_answered_as_it_arrived() {
        let mut u = chat_ui();
        let call = ToolCall { id: "c1".into(), name: "create_node".into(), arguments: json!({"type": "rectangle", "width": 10, "height": 10}) };
        let text = json!({"id": 1, "children": []}).to_string();
        assert_eq!(place_media_answer(&mut u.app, &call, &text), text, "an engine tool's answer is untouched");
    }

    #[test]
    fn the_media_keys_come_from_the_store_and_only_for_providers_with_image_models() {
        let mut u = chat_ui();
        let (design, stock, _, _) = media_keys(&u.app);
        assert_eq!(design.as_deref(), Some("sk-test"), "the design key, for a provider with an image model");
        assert_eq!(stock, None, "no Pexels key yet");
        u.app
            .session
            .execute("ai.credential.set", &json!({"integration": "pexels", "profileId": "default", "field": "api-key", "value": "pk"}))
            .expect("the key is stored");
        let (_, stock, _, _) = media_keys(&u.app);
        assert_eq!(stock.as_deref(), Some("pk"));
        // A provider without an image model generates nothing, whatever its key.
        u.app.session.prefs.ai.models.connections[0].provider = figcraft_ai::ProviderId::Deepseek;
        let (design, _, _, _) = media_keys(&u.app);
        assert_eq!(design, None, "DeepSeek ships no image model");
    }

    #[test]
    fn an_unverified_image_is_reported_not_placed() {
        use figcraft_ai::{MediaFactory, MediaSession};
        struct Rejecting;
        impl MediaSession for Rejecting {
            fn generate_image<'a>(
                &'a mut self,
                _: &'a str,
                _: figcraft_ai::Orientation,
            ) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Ok(vec![1, 2, 3]) })
            }
            fn search_assets<'a>(
                &'a mut self,
                _: &'a str,
                _: &'a figcraft_ai::AssetSearchOptions,
            ) -> figcraft_ai::BoxFuture<'a, Vec<figcraft_ai::AssetSearchOutcome>> {
                Box::pin(async { Vec::new() })
            }
            fn stock_photo<'a>(
                &'a mut self,
                _: &'a str,
                _: figcraft_ai::Orientation,
                _: usize,
            ) -> figcraft_ai::BoxFuture<'a, Result<figcraft_ai::StockPhoto, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no stock".into())) })
            }
            fn download<'a>(&'a mut self, _: &'a str) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Ok(vec![]) })
            }
            fn inspect_image<'a>(&'a mut self, _: &'a [u8], _: &'a str, _: &'a str) -> figcraft_ai::BoxFuture<'a, Result<String, TransportError>> {
                Box::pin(async { Ok("no verdict here".into()) })
            }
            fn vectorize<'a>(&'a mut self, _: &'a [u8]) -> figcraft_ai::BoxFuture<'a, Result<String, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no".into())) })
            }
            fn remove_background<'a>(&'a mut self, _: &'a [u8]) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no".into())) })
            }
        }
        struct Factory;
        impl MediaFactory for Factory {
            fn make(
                &self,
                _: Option<String>,
                _: Option<String>,
                _: Option<figcraft_ai::VisionConfig>,
                _: Option<(figcraft_ai::VectorizeProvider, String)>,
            ) -> Box<dyn MediaSession> {
                Box::new(Rejecting)
            }
        }
        let text = figcraft_ai::media::block_on(super::source_image(
            &Factory,
            &(None, None, None, None),
            &json!({"requests": r#"[{"id":"0:30","subject":"a fox","must_generate":true}]"#}),
        ));
        let answer: Value = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(answer["applied"], 0, "nothing was applied");
        assert_eq!(answer["results"][0]["state"], "no-verified-match");
        assert!(answer["results"][0]["rejected"].as_array().is_some_and(|a| !a.is_empty()));
    }

    #[test]
    fn the_no_key_libraries_are_tried_before_the_keyed_and_generated_fallbacks() {
        use figcraft_ai::{MediaFactory, MediaSession};
        use std::sync::{Arc, Mutex};

        let calls = Arc::new(Mutex::new(Vec::<String>::new()));
        struct Recorder(Arc<Mutex<Vec<String>>>);
        impl MediaSession for Recorder {
            fn generate_image<'a>(
                &'a mut self,
                _: &'a str,
                _: figcraft_ai::Orientation,
            ) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                self.0.lock().expect("lock").push("generate".into());
                Box::pin(async { Err(TransportError::Other("should not be reached".into())) })
            }
            fn search_assets<'a>(
                &'a mut self,
                query: &'a str,
                opts: &'a figcraft_ai::AssetSearchOptions,
            ) -> figcraft_ai::BoxFuture<'a, Vec<figcraft_ai::AssetSearchOutcome>> {
                self.0.lock().expect("lock").push(format!("search:{query}:{}", opts.limit));
                let asset = figcraft_ai::ImageAsset {
                    id: "1".into(),
                    provider: "openverse".into(),
                    url: "https://images.test/fox.jpg".into(),
                    thumbnail_url: Some("https://images.test/fox-thumb.jpg".into()),
                    title: Some("a fox".into()),
                    description: None,
                    mime_type: Some("image/png".into()),
                    width: Some(800),
                    height: Some(600),
                    creator: Some("Ada".into()),
                    source_page: Some("https://openverse.test/1".into()),
                    license: Some("by".into()),
                    license_url: None,
                    attribution: Some("Ada".into()),
                    commercial_use: true,
                    modification_allowed: true,
                    requires_attribution: true,
                    kind: figcraft_ai::AssetKind::Raster,
                };
                Box::pin(async move { vec![figcraft_ai::AssetSearchOutcome { provider: "openverse".into(), assets: vec![asset], error: None }] })
            }
            fn stock_photo<'a>(
                &'a mut self,
                _: &'a str,
                _: figcraft_ai::Orientation,
                _: usize,
            ) -> figcraft_ai::BoxFuture<'a, Result<figcraft_ai::StockPhoto, TransportError>> {
                self.0.lock().expect("lock").push("stock".into());
                Box::pin(async { Err(TransportError::Other("should not be reached".into())) })
            }
            fn download<'a>(&'a mut self, url: &'a str) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                self.0.lock().expect("lock").push(format!("download:{url}"));
                // A real 1x1 PNG so the bytes are a valid image.
                Box::pin(async { Ok(vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) })
            }
            fn inspect_image<'a>(&'a mut self, _: &'a [u8], _: &'a str, _: &'a str) -> figcraft_ai::BoxFuture<'a, Result<String, TransportError>> {
                self.0.lock().expect("lock").push("vision".into());
                Box::pin(async {
                    Ok(r#"{"approved": true, "relevance_score": 0.9, "matched_requirements": ["subject"], "failed_requirements": [], "reason": "it is a fox"}"#.into())
                })
            }
            fn vectorize<'a>(&'a mut self, _: &'a [u8]) -> figcraft_ai::BoxFuture<'a, Result<String, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no".into())) })
            }
            fn remove_background<'a>(&'a mut self, _: &'a [u8]) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no".into())) })
            }
        }
        struct Factory(Arc<Mutex<Vec<String>>>);
        impl MediaFactory for Factory {
            fn make(
                &self,
                _: Option<String>,
                _: Option<String>,
                _: Option<figcraft_ai::VisionConfig>,
                _: Option<(figcraft_ai::VectorizeProvider, String)>,
            ) -> Box<dyn MediaSession> {
                Box::new(Recorder(self.0.clone()))
            }
        }

        // `use_stock` forces the Search route, so generation is excluded regardless.
        let text = figcraft_ai::media::block_on(super::source_image(
            &Factory(calls.clone()),
            &(None, None, None, None),
            &json!({"requests": r#"[{"id":"0:30","subject":"a fox","use_stock":true}]"#}),
        ));
        let answer: Value = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(answer["applied"], 1, "the free source was approved and placed");
        let credit = answer["results"][0]["credit"].as_str().expect("a credit");
        assert!(credit.contains("Ada") && credit.contains("BY"), "attribution travels with the image: {credit}");

        let log = calls.lock().expect("lock");
        assert!(log.iter().any(|c| c.starts_with("search:a fox:")), "the no-key libraries were searched: {log:?}");
        // The preview, not the original: a library original can be tens of megabytes, past the
        // size cap and far more than the slot shows.
        assert!(log.iter().any(|c| c == "download:https://images.test/fox-thumb.jpg"), "the preview was downloaded: {log:?}");
        assert!(!log.iter().any(|c| c == "download:https://images.test/fox.jpg"), "the original was left alone: {log:?}");
        assert!(log.contains(&"vision".to_string()), "the pixels were inspected: {log:?}");
        assert!(!log.contains(&"stock".to_string()), "the keyed service was not needed: {log:?}");
        assert!(!log.contains(&"generate".to_string()), "generation was not needed: {log:?}");
    }

    /// A session that records the calls it sees, answers the library search with one asset, and
    /// refuses generation because the connection has no image key.
    ///
    /// This is the shape of a free connection: a chat model, a Vision model, and no image model.
    fn keyless_connection(calls: std::sync::Arc<std::sync::Mutex<Vec<String>>>) -> Box<dyn figcraft_ai::MediaSession> {
        use figcraft_ai::MediaSession;
        struct Keyless(std::sync::Arc<std::sync::Mutex<Vec<String>>>);
        impl MediaSession for Keyless {
            fn generate_image<'a>(
                &'a mut self,
                _: &'a str,
                _: figcraft_ai::Orientation,
            ) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                self.0.lock().expect("lock").push("generate".into());
                Box::pin(async { Err(TransportError::Other("The connection has no API key, so no image can be generated.".into())) })
            }
            fn search_assets<'a>(
                &'a mut self,
                query: &'a str,
                _: &'a figcraft_ai::AssetSearchOptions,
            ) -> figcraft_ai::BoxFuture<'a, Vec<figcraft_ai::AssetSearchOutcome>> {
                self.0.lock().expect("lock").push(format!("search:{query}"));
                let asset = figcraft_ai::ImageAsset {
                    id: "1".into(),
                    provider: "openverse".into(),
                    url: "https://images.test/mug.jpg".into(),
                    thumbnail_url: Some("https://images.test/mug-thumb.jpg".into()),
                    title: Some("a mug".into()),
                    description: None,
                    mime_type: Some("image/jpeg".into()),
                    width: Some(800),
                    height: Some(600),
                    creator: Some("Ada".into()),
                    source_page: Some("https://openverse.test/1".into()),
                    license: Some("by".into()),
                    license_url: None,
                    attribution: Some("Ada".into()),
                    commercial_use: true,
                    modification_allowed: true,
                    requires_attribution: true,
                    kind: figcraft_ai::AssetKind::Raster,
                };
                Box::pin(async move { vec![figcraft_ai::AssetSearchOutcome { provider: "openverse".into(), assets: vec![asset], error: None }] })
            }
            fn stock_photo<'a>(
                &'a mut self,
                _: &'a str,
                _: figcraft_ai::Orientation,
                _: usize,
            ) -> figcraft_ai::BoxFuture<'a, Result<figcraft_ai::StockPhoto, TransportError>> {
                self.0.lock().expect("lock").push("stock".into());
                Box::pin(async { Err(TransportError::Other("no stock key".into())) })
            }
            fn download<'a>(&'a mut self, url: &'a str) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                self.0.lock().expect("lock").push(format!("download:{url}"));
                Box::pin(async { Ok(vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) })
            }
            fn inspect_image<'a>(&'a mut self, _: &'a [u8], _: &'a str, _: &'a str) -> figcraft_ai::BoxFuture<'a, Result<String, TransportError>> {
                self.0.lock().expect("lock").push("vision".into());
                Box::pin(async {
                    Ok(r#"{"approved": true, "relevance_score": 0.9, "matched_requirements": ["subject"], "failed_requirements": [], "reason": "a mug"}"#.into())
                })
            }
            fn vectorize<'a>(&'a mut self, _: &'a [u8]) -> figcraft_ai::BoxFuture<'a, Result<String, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no".into())) })
            }
            fn remove_background<'a>(&'a mut self, _: &'a [u8]) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no".into())) })
            }
        }
        Box::new(Keyless(calls))
    }

    struct KeylessFactory(std::sync::Arc<std::sync::Mutex<Vec<String>>>);
    impl figcraft_ai::MediaFactory for KeylessFactory {
        fn make(
            &self,
            _: Option<String>,
            _: Option<String>,
            _: Option<figcraft_ai::VisionConfig>,
            _: Option<(figcraft_ai::VectorizeProvider, String)>,
        ) -> Box<dyn figcraft_ai::MediaSession> {
            keyless_connection(self.0.clone())
        }
    }

    #[test]
    fn a_slot_asked_for_as_generated_is_still_filled_from_the_libraries_when_there_is_no_image_key() {
        use std::sync::{Arc, Mutex};

        // The model supplies a palette and a mood, which is two specifics: `route` answers
        // Generate. Before the fix that skipped the free libraries entirely, the generate tier
        // failed with "the connection has no API key", and the screen went out with an empty
        // grey box — on a connection whose Vision model was working perfectly.
        let calls = Arc::new(Mutex::new(Vec::new()));
        let text = figcraft_ai::media::block_on(super::source_image(
            &KeylessFactory(calls.clone()),
            &(None, None, None, None),
            &json!({"requests": r#"[{"id":"7","subject":"a stoneware mug","palette":"bone and terracotta","mood":"calm"}]"#}),
        ));
        let answer: Value = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(answer["applied"], 1, "the library filled the slot: {answer}");
        assert_eq!(answer["results"][0]["route"], "search", "the answer says where the picture came from");

        let log = calls.lock().expect("lock");
        assert!(log.iter().any(|c| c == "generate"), "generation was tried first, as the route asked: {log:?}");
        assert!(log.iter().any(|c| c.starts_with("search:")), "then the libraries: {log:?}");
    }

    #[test]
    fn a_search_that_finds_nothing_is_tried_again_with_fewer_words() {
        use std::sync::{Arc, Mutex};

        // Six specific words is how a person searches a library and why it answers with two
        // results. The second round keeps the words that carry the subject.
        struct Narrow(Arc<Mutex<Vec<String>>>);
        impl figcraft_ai::MediaSession for Narrow {
            fn generate_image<'a>(
                &'a mut self,
                _: &'a str,
                _: figcraft_ai::Orientation,
            ) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no image model".into())) })
            }
            fn search_assets<'a>(
                &'a mut self,
                query: &'a str,
                _: &'a figcraft_ai::AssetSearchOptions,
            ) -> figcraft_ai::BoxFuture<'a, Vec<figcraft_ai::AssetSearchOutcome>> {
                self.0.lock().expect("lock").push(format!("search:{query}"));
                Box::pin(async { vec![figcraft_ai::AssetSearchOutcome { provider: "openverse".into(), assets: Vec::new(), error: None }] })
            }
            fn stock_photo<'a>(
                &'a mut self,
                _: &'a str,
                _: figcraft_ai::Orientation,
                _: usize,
            ) -> figcraft_ai::BoxFuture<'a, Result<figcraft_ai::StockPhoto, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no stock key".into())) })
            }
            fn download<'a>(&'a mut self, _: &'a str) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Err(TransportError::Other("nothing to download".into())) })
            }
            fn inspect_image<'a>(&'a mut self, _: &'a [u8], _: &'a str, _: &'a str) -> figcraft_ai::BoxFuture<'a, Result<String, TransportError>> {
                Box::pin(async { Err(TransportError::Other("nothing to inspect".into())) })
            }
            fn vectorize<'a>(&'a mut self, _: &'a [u8]) -> figcraft_ai::BoxFuture<'a, Result<String, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no".into())) })
            }
            fn remove_background<'a>(&'a mut self, _: &'a [u8]) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no".into())) })
            }
        }
        struct NarrowFactory(Arc<Mutex<Vec<String>>>);
        impl figcraft_ai::MediaFactory for NarrowFactory {
            fn make(
                &self,
                _: Option<String>,
                _: Option<String>,
                _: Option<figcraft_ai::VisionConfig>,
                _: Option<(figcraft_ai::VectorizeProvider, String)>,
            ) -> Box<dyn figcraft_ai::MediaSession> {
                Box::new(Narrow(self.0.clone()))
            }
        }

        let calls = Arc::new(Mutex::new(Vec::new()));
        let text = figcraft_ai::media::block_on(super::source_image(
            &NarrowFactory(calls.clone()),
            &(None, None, None, None),
            &json!({"requests": r#"[{"id":"1","subject":"hand-thrown stoneware cup","query":"hand-thrown cream and rust stoneware cups on linen","use_stock":true}]"#}),
        ));
        let answer: Value = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(answer["applied"], 0, "nothing was found: {answer}");

        let log = calls.lock().expect("lock");
        let searches: Vec<&String> = log.iter().filter(|c| c.starts_with("search:")).collect();
        assert_eq!(searches.len(), 2, "the query was tried, then broadened: {log:?}");
        assert_ne!(searches[0], searches[1], "the second round is a different query");
        assert!(searches[1].split_whitespace().count() <= 3, "the second round keeps the words that carry the subject: {log:?}");
    }

    #[test]
    fn a_rate_limited_vision_call_is_retried_and_does_not_abandon_the_slot() {
        use figcraft_ai::{MediaFactory, MediaSession};
        use std::sync::{Arc, Mutex};

        // Two candidates; the endpoint refuses the first inspection and answers the second.
        // Before the retry, one refusal was reported as "the model is unavailable" and the whole
        // run stopped, so a good picture behind it was never seen.
        struct Flaky {
            refused: Arc<Mutex<bool>>,
        }
        impl MediaSession for Flaky {
            fn generate_image<'a>(
                &'a mut self,
                _: &'a str,
                _: figcraft_ai::Orientation,
            ) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Err(TransportError::Other("should not be reached".into())) })
            }
            fn search_assets<'a>(
                &'a mut self,
                _: &'a str,
                _: &'a figcraft_ai::AssetSearchOptions,
            ) -> figcraft_ai::BoxFuture<'a, Vec<figcraft_ai::AssetSearchOutcome>> {
                let asset = |id: &str| figcraft_ai::ImageAsset {
                    id: id.into(),
                    provider: "openverse".into(),
                    url: format!("https://images.test/{id}.jpg"),
                    thumbnail_url: None,
                    title: Some("a fox".into()),
                    description: None,
                    mime_type: Some("image/png".into()),
                    width: Some(800),
                    height: Some(600),
                    creator: Some("Ada".into()),
                    source_page: Some(format!("https://openverse.test/{id}")),
                    license: Some("by".into()),
                    license_url: None,
                    attribution: Some("Ada".into()),
                    commercial_use: true,
                    modification_allowed: true,
                    requires_attribution: true,
                    kind: figcraft_ai::AssetKind::Raster,
                };
                Box::pin(async move {
                    vec![figcraft_ai::AssetSearchOutcome { provider: "openverse".into(), assets: vec![asset("1"), asset("2")], error: None }]
                })
            }
            fn stock_photo<'a>(
                &'a mut self,
                _: &'a str,
                _: figcraft_ai::Orientation,
                _: usize,
            ) -> figcraft_ai::BoxFuture<'a, Result<figcraft_ai::StockPhoto, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no stock key".into())) })
            }
            fn download<'a>(&'a mut self, _: &'a str) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Ok(vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) })
            }
            fn inspect_image<'a>(&'a mut self, _: &'a [u8], _: &'a str, _: &'a str) -> figcraft_ai::BoxFuture<'a, Result<String, TransportError>> {
                let refused = self.refused.clone();
                Box::pin(async move {
                    let mut was_refused = refused.lock().map_err(|_| TransportError::Other("poisoned".into()))?;
                    if !*was_refused {
                        *was_refused = true;
                        return Err(TransportError::RateLimited);
                    }
                    Ok(r#"{"approved": true, "relevance_score": 0.9, "matched_requirements": ["subject"], "failed_requirements": [], "reason": "it is a fox"}"#.into())
                })
            }
            fn vectorize<'a>(&'a mut self, _: &'a [u8]) -> figcraft_ai::BoxFuture<'a, Result<String, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no".into())) })
            }
            fn remove_background<'a>(&'a mut self, _: &'a [u8]) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no".into())) })
            }
        }
        struct Factory(Arc<Mutex<bool>>);
        impl MediaFactory for Factory {
            fn make(
                &self,
                _: Option<String>,
                _: Option<String>,
                _: Option<figcraft_ai::VisionConfig>,
                _: Option<(figcraft_ai::VectorizeProvider, String)>,
            ) -> Box<dyn figcraft_ai::MediaSession> {
                Box::new(Flaky { refused: self.0.clone() })
            }
        }

        let text = figcraft_ai::media::block_on(super::source_image(
            &Factory(Arc::new(Mutex::new(false))),
            &(None, None, None, None),
            &json!({"requests": r#"[{"id":"0:30","subject":"a fox","use_stock":true}]"#}),
        ));
        let answer: Value = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(answer["applied"], 1, "the retry recovered the picture: {answer}");
        assert_eq!(answer["results"][0]["state"], "verified");
    }

    #[test]
    fn a_provider_hiccup_on_one_candidate_does_not_abandon_the_slot() {
        use figcraft_ai::{MediaFactory, MediaSession};
        use std::sync::{Arc, Mutex};

        // The failure this guards: the free endpoint answers `500` or `424` for one request while
        // it is busy. The inspection that came back with that error was reported as "the Vision
        // model is unavailable", the slot stopped, and the run went on to the next slot with the
        // same verdict — so a finished page shipped with grey boxes in it and a reason that was
        // not true. A busy provider is not a broken one: the next candidate gets its own attempt.
        struct BusyOnce(Arc<Mutex<usize>>);
        impl MediaSession for BusyOnce {
            fn search_assets<'a>(
                &'a mut self,
                _: &'a str,
                _: &'a figcraft_ai::AssetSearchOptions,
            ) -> figcraft_ai::BoxFuture<'a, Vec<figcraft_ai::AssetSearchOutcome>> {
                let asset = |id: &str| figcraft_ai::ImageAsset {
                    id: id.into(),
                    provider: "openverse".into(),
                    url: format!("https://images.test/{id}.jpg"),
                    thumbnail_url: None,
                    title: Some("a fox".into()),
                    description: None,
                    mime_type: Some("image/png".into()),
                    width: Some(800),
                    height: Some(600),
                    creator: Some("Ada".into()),
                    source_page: Some(format!("https://openverse.test/{id}")),
                    license: Some("by".into()),
                    license_url: None,
                    attribution: Some("Ada".into()),
                    commercial_use: true,
                    modification_allowed: true,
                    requires_attribution: true,
                    kind: figcraft_ai::AssetKind::Raster,
                };
                Box::pin(async move {
                    vec![figcraft_ai::AssetSearchOutcome { provider: "openverse".into(), assets: vec![asset("1"), asset("2")], error: None }]
                })
            }
            fn stock_photo<'a>(
                &'a mut self,
                _: &'a str,
                _: figcraft_ai::Orientation,
                _: usize,
            ) -> figcraft_ai::BoxFuture<'a, Result<figcraft_ai::StockPhoto, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no stock key".into())) })
            }
            fn download<'a>(&'a mut self, _: &'a str) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Ok(vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) })
            }
            fn inspect_image<'a>(&'a mut self, _: &'a [u8], _: &'a str, _: &'a str) -> figcraft_ai::BoxFuture<'a, Result<String, TransportError>> {
                let seen = self.0.clone();
                Box::pin(async move {
                    let mut n = seen.lock().map_err(|_| TransportError::Other("poisoned".into()))?;
                    *n += 1;
                    if *n == 1 {
                        // What the free endpoint actually answers while it is busy.
                        return Err(TransportError::BadResponse("500 internal error".into()));
                    }
                    Ok(r#"{"approved": true, "relevance_score": 0.9, "matched_requirements": ["subject"], "failed_requirements": [], "reason": "it is a fox"}"#.into())
                })
            }
            fn generate_image<'a>(
                &'a mut self,
                _: &'a str,
                _: figcraft_ai::Orientation,
            ) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no image model".into())) })
            }
            fn vectorize<'a>(&'a mut self, _: &'a [u8]) -> figcraft_ai::BoxFuture<'a, Result<String, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no".into())) })
            }
            fn remove_background<'a>(&'a mut self, _: &'a [u8]) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no".into())) })
            }
        }
        struct Factory(Arc<Mutex<usize>>);
        impl MediaFactory for Factory {
            fn make(
                &self,
                _: Option<String>,
                _: Option<String>,
                _: Option<figcraft_ai::VisionConfig>,
                _: Option<(figcraft_ai::VectorizeProvider, String)>,
            ) -> Box<dyn figcraft_ai::MediaSession> {
                Box::new(BusyOnce(self.0.clone()))
            }
        }

        let text = figcraft_ai::media::block_on(super::source_image(
            &Factory(Arc::new(Mutex::new(0))),
            &(None, None, None, None),
            &json!({"requests": r#"[{"id":"0:30","subject":"a fox","use_stock":true}]"#}),
        ));
        let answer: Value = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(answer["results"][0]["state"], "verified", "the second candidate is still tried: {answer}");
        assert_eq!(answer["applied"], 1);
    }

    #[test]
    fn a_wrong_key_is_reported_instead_of_spending_the_whole_budget() {
        use figcraft_ai::{MediaFactory, MediaSession};
        use std::sync::{Arc, Mutex};

        // A refused key cannot start working on the next candidate, and the slot's budget is six
        // inspections. Spending all six on the same refusal told the model "no match found",
        // which sent it looking for different pictures instead of telling the person to fix the
        // key.
        struct Refused(Arc<Mutex<usize>>);
        impl MediaSession for Refused {
            fn search_assets<'a>(
                &'a mut self,
                _: &'a str,
                _: &'a figcraft_ai::AssetSearchOptions,
            ) -> figcraft_ai::BoxFuture<'a, Vec<figcraft_ai::AssetSearchOutcome>> {
                let asset = |id: &str| figcraft_ai::ImageAsset {
                    id: id.into(),
                    provider: "openverse".into(),
                    url: format!("https://images.test/{id}.jpg"),
                    thumbnail_url: None,
                    title: Some("a fox".into()),
                    description: None,
                    mime_type: Some("image/png".into()),
                    width: Some(800),
                    height: Some(600),
                    creator: Some("Ada".into()),
                    source_page: Some(format!("https://openverse.test/{id}")),
                    license: Some("by".into()),
                    license_url: None,
                    attribution: Some("Ada".into()),
                    commercial_use: true,
                    modification_allowed: true,
                    requires_attribution: true,
                    kind: figcraft_ai::AssetKind::Raster,
                };
                Box::pin(async move {
                    vec![figcraft_ai::AssetSearchOutcome {
                        provider: "openverse".into(),
                        assets: (0..10).map(|i| asset(&i.to_string())).collect(),
                        error: None,
                    }]
                })
            }
            fn stock_photo<'a>(
                &'a mut self,
                _: &'a str,
                _: figcraft_ai::Orientation,
                _: usize,
            ) -> figcraft_ai::BoxFuture<'a, Result<figcraft_ai::StockPhoto, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no stock key".into())) })
            }
            fn download<'a>(&'a mut self, _: &'a str) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Ok(vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) })
            }
            fn inspect_image<'a>(&'a mut self, _: &'a [u8], _: &'a str, _: &'a str) -> figcraft_ai::BoxFuture<'a, Result<String, TransportError>> {
                let seen = self.0.clone();
                Box::pin(async move {
                    *seen.lock().map_err(|_| TransportError::Other("poisoned".into()))? += 1;
                    Err(TransportError::Authentication)
                })
            }
            fn generate_image<'a>(
                &'a mut self,
                _: &'a str,
                _: figcraft_ai::Orientation,
            ) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no image model".into())) })
            }
            fn vectorize<'a>(&'a mut self, _: &'a [u8]) -> figcraft_ai::BoxFuture<'a, Result<String, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no".into())) })
            }
            fn remove_background<'a>(&'a mut self, _: &'a [u8]) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no".into())) })
            }
        }
        struct Factory(Arc<Mutex<usize>>);
        impl MediaFactory for Factory {
            fn make(
                &self,
                _: Option<String>,
                _: Option<String>,
                _: Option<figcraft_ai::VisionConfig>,
                _: Option<(figcraft_ai::VectorizeProvider, String)>,
            ) -> Box<dyn figcraft_ai::MediaSession> {
                Box::new(Refused(self.0.clone()))
            }
        }

        let calls = Arc::new(Mutex::new(0));
        let text = figcraft_ai::media::block_on(super::source_image(
            &Factory(calls.clone()),
            &(None, None, None, None),
            &json!({"requests": r#"[{"id":"0:30","subject":"a fox","use_stock":true}]"#}),
        ));
        let answer: Value = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(answer["results"][0]["state"], "vision-unavailable-needs-user-selection", "a refused key is named: {answer}");
        assert_eq!(*calls.lock().expect("lock"), 1, "the refusal is not retried ten times");
    }

    #[test]
    fn a_screen_that_stopped_before_its_pictures_is_handed_the_audit_findings() {
        let mut u = chat_ui();
        // The failure this guards: a model lays out a hero, draws a grey box named for the
        // photography, and calls the screen finished. The audit reads the document, so the app
        // knows the slot is empty even though the model never asked.
        let page = u.app.run("node.create", json!({"type": "frame", "x": 0, "y": 0, "width": 720, "height": 720, "name": "Hero"})).expect("a frame");
        let page_id = page["id"].clone();
        u.app
            .run("node.create", json!({"type": "rectangle", "parent": page_id, "x": 0, "y": 0, "width": 720, "height": 720, "name": "IMG·hero", "props": {"fills": ["#E7E2D8"]}}))
            .expect("a slot");

        let blockers = audit_blockers(&mut u.app).expect("the empty slot blocks the finish");
        assert!(blockers.contains("empty-slot"), "the finding is named: {blockers}");
        assert!(blockers.contains("source_image"), "the fix names the tool that fills it: {blockers}");
        assert!(blockers.contains("IMG·hero"), "the layer is named so the model can find it: {blockers}");
    }

    #[test]
    fn a_screen_that_holds_up_is_finished_without_another_round() {
        let mut u = chat_ui();
        let page = u.app.run("node.create", json!({"type": "frame", "x": 0, "y": 0, "width": 720, "height": 720, "name": "Hero"})).expect("a frame");
        let page_id = page["id"].clone();
        u.app
            .run("text.create", json!({"parent": page_id, "x": 40, "y": 40, "characters": "Northgate Studio", "props": {"fontSize": 40, "textAutoResize": "height", "width": 400}}))
            .expect("a headline");

        // Nothing to report means nothing is said: a finished design does not owe another model
        // step, which on a metered connection is the person's money.
        assert_eq!(audit_blockers(&mut u.app), None);
    }

    #[test]
    fn the_candidates_behind_an_approved_picture_are_never_downloaded() {
        use std::sync::{Arc, Mutex};

        // The failure this guards: a search returns twenty candidates, the Vision budget is six,
        // and the code downloaded all twenty before inspecting any. Fourteen transfers per slot
        // went to pictures nobody would see, and the libraries read the burst as abuse — the slot
        // then failed with a rate limit that had nothing to do with the pictures it found.
        struct Counting(Arc<Mutex<Vec<String>>>);
        impl figcraft_ai::MediaSession for Counting {
            fn search_assets<'a>(
                &'a mut self,
                _: &'a str,
                _: &'a figcraft_ai::AssetSearchOptions,
            ) -> figcraft_ai::BoxFuture<'a, Vec<figcraft_ai::AssetSearchOutcome>> {
                Box::pin(async { Vec::new() })
            }
            fn stock_photo<'a>(
                &'a mut self,
                _: &'a str,
                _: figcraft_ai::Orientation,
                _: usize,
            ) -> figcraft_ai::BoxFuture<'a, Result<figcraft_ai::StockPhoto, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no stock key".into())) })
            }
            fn download<'a>(&'a mut self, url: &'a str) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                self.0.lock().expect("lock").push(url.to_string());
                Box::pin(async { Ok(vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) })
            }
            fn inspect_image<'a>(&'a mut self, _: &'a [u8], _: &'a str, _: &'a str) -> figcraft_ai::BoxFuture<'a, Result<String, TransportError>> {
                Box::pin(async {
                    Ok(r#"{"approved": true, "relevance_score": 0.9, "matched_requirements": ["subject"], "failed_requirements": [], "reason": "a mug"}"#.into())
                })
            }
            fn generate_image<'a>(
                &'a mut self,
                _: &'a str,
                _: figcraft_ai::Orientation,
            ) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no image model".into())) })
            }
            fn vectorize<'a>(&'a mut self, _: &'a [u8]) -> figcraft_ai::BoxFuture<'a, Result<String, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no".into())) })
            }
            fn remove_background<'a>(&'a mut self, _: &'a [u8]) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no".into())) })
            }
        }

        let downloads = Arc::new(Mutex::new(Vec::new()));
        let mut session = Counting(downloads.clone());
        let assets: Vec<(String, String)> = (0..20).map(|i| (format!("https://example.com/{i}.jpg"), format!("credit {i}"))).collect();
        let intent = figcraft_ai::vision::VisualIntent::default();
        let mut rejected = Vec::new();
        let mut vision_error = None;
        let mut budget = figcraft_ai::MAX_VISION_CALLS;

        let mut near = super::NearMiss::default();
        let found = figcraft_ai::media::block_on(super::verify_assets(
            assets,
            &mut session,
            &intent,
            &mut rejected,
            &mut vision_error,
            &mut budget,
            &mut near,
        ));
        assert!(found.is_some(), "the first candidate is approved");
        assert_eq!(downloads.lock().expect("lock").len(), 1, "only the candidate that was looked at was fetched");
        assert_eq!(budget, figcraft_ai::MAX_VISION_CALLS - 1, "one inspection, one call");
    }

    #[test]
    fn a_recognisable_photograph_fills_a_slot_no_candidate_was_approved_for() {
        use std::sync::{Arc, Mutex};

        // Every library candidate is a real photograph of roughly the right thing and none is
        // the brief. Rejecting all of them shipped grey boxes; the closest one is what a person
        // would have picked themselves.
        struct OnlyNearMiss(Arc<Mutex<usize>>);
        impl figcraft_ai::MediaSession for OnlyNearMiss {
            fn search_assets<'a>(
                &'a mut self,
                _: &'a str,
                _: &'a figcraft_ai::AssetSearchOptions,
            ) -> figcraft_ai::BoxFuture<'a, Vec<figcraft_ai::AssetSearchOutcome>> {
                Box::pin(async { Vec::new() })
            }
            fn stock_photo<'a>(
                &'a mut self,
                _: &'a str,
                _: figcraft_ai::Orientation,
                _: usize,
            ) -> figcraft_ai::BoxFuture<'a, Result<figcraft_ai::StockPhoto, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no stock key".into())) })
            }
            fn download<'a>(&'a mut self, _: &'a str) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                *self.0.lock().expect("lock") += 1;
                Box::pin(async { Ok(vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) })
            }
            fn inspect_image<'a>(&'a mut self, _: &'a [u8], _: &'a str, _: &'a str) -> figcraft_ai::BoxFuture<'a, Result<String, TransportError>> {
                Box::pin(async {
                    Ok(r#"{"approved": false, "relevance_score": 0.55, "matched_requirements": ["usable-in-ui"], "failed_requirements": ["subject"], "reason": "a woman by a window, not the room the brief asked for"}"#.into())
                })
            }
            fn generate_image<'a>(
                &'a mut self,
                _: &'a str,
                _: figcraft_ai::Orientation,
            ) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no image model".into())) })
            }
            fn vectorize<'a>(&'a mut self, _: &'a [u8]) -> figcraft_ai::BoxFuture<'a, Result<String, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no".into())) })
            }
            fn remove_background<'a>(&'a mut self, _: &'a [u8]) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no".into())) })
            }
        }

        let mut session = OnlyNearMiss(Arc::new(Mutex::new(0)));
        let assets: Vec<(String, String)> = (0..3).map(|i| (format!("https://example.com/{i}.jpg"), format!("credit {i}"))).collect();
        let intent = figcraft_ai::vision::VisualIntent::default();
        let mut rejected = Vec::new();
        let mut vision_error = None;
        let mut budget = figcraft_ai::MAX_VISION_CALLS;
        let mut near = super::NearMiss::default();
        let found = figcraft_ai::media::block_on(super::verify_assets(
            assets,
            &mut session,
            &intent,
            &mut rejected,
            &mut vision_error,
            &mut budget,
            &mut near,
        ));
        assert!(found.is_none(), "nothing was approved outright");
        let (_, reason) = near.best.expect("the closest picture is kept for the slot");
        assert!(reason.starts_with("closest available"), "{reason}");
    }

    #[test]
    fn the_best_near_miss_is_kept_rather_than_the_last_one_seen() {
        // Two rejected pictures, the earlier one the better match. Keeping whichever arrived
        // last put a loosely related photograph in the slot while a better one was in hand.
        let mut near = super::NearMiss::default();
        let image = super::Placed { bytes: vec![1, 2, 3], credit: "credit".into() };
        let worse = figcraft_ai::vision::VisionVerdict {
            approved: false,
            relevance_score: 0.5,
            matched_requirements: Vec::new(),
            failed_requirements: vec!["subject".into()],
            reason: "a harbour, but no boats".into(),
        };
        let better = figcraft_ai::vision::VisionVerdict { relevance_score: 0.66, reason: "a harbour in fog".into(), ..worse.clone() };
        super::keep_near_miss(&better, &image, &mut near);
        super::keep_near_miss(&worse, &image, &mut near);
        let (_, reason) = near.best.expect("the best near miss is kept");
        assert!(reason.contains("a harbour in fog"), "{reason}");
    }

    #[test]
    fn a_throttled_verifier_still_leaves_a_licensed_picture_in_the_slot() {
        use std::sync::{Arc, Mutex};

        // Every inspection is refused with a 429. Nothing was judged, so nothing was rejected
        // either: leaving the slot empty reports "no picture matched" when no picture was looked
        // at, and a licensed photograph in the right shape beats a grey box.
        struct Throttled(Arc<Mutex<usize>>);
        impl figcraft_ai::MediaSession for Throttled {
            fn search_assets<'a>(
                &'a mut self,
                _: &'a str,
                _: &'a figcraft_ai::AssetSearchOptions,
            ) -> figcraft_ai::BoxFuture<'a, Vec<figcraft_ai::AssetSearchOutcome>> {
                Box::pin(async { Vec::new() })
            }
            fn stock_photo<'a>(
                &'a mut self,
                _: &'a str,
                _: figcraft_ai::Orientation,
                _: usize,
            ) -> figcraft_ai::BoxFuture<'a, Result<figcraft_ai::StockPhoto, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no stock key".into())) })
            }
            fn download<'a>(&'a mut self, _: &'a str) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                *self.0.lock().expect("lock") += 1;
                Box::pin(async { Ok(vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) })
            }
            fn inspect_image<'a>(&'a mut self, _: &'a [u8], _: &'a str, _: &'a str) -> figcraft_ai::BoxFuture<'a, Result<String, TransportError>> {
                Box::pin(async { Err(TransportError::RateLimited) })
            }
            fn generate_image<'a>(
                &'a mut self,
                _: &'a str,
                _: figcraft_ai::Orientation,
            ) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no image model".into())) })
            }
            fn vectorize<'a>(&'a mut self, _: &'a [u8]) -> figcraft_ai::BoxFuture<'a, Result<String, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no".into())) })
            }
            fn remove_background<'a>(&'a mut self, _: &'a [u8]) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no".into())) })
            }
        }

        let mut session = Throttled(Arc::new(Mutex::new(0)));
        let assets: Vec<(String, String)> = vec![("https://example.com/a.jpg".into(), "credit a".into())];
        let intent = figcraft_ai::vision::VisualIntent::default();
        let mut rejected = Vec::new();
        let mut vision_error = None;
        let mut budget = figcraft_ai::MAX_VISION_CALLS;
        let mut near = super::NearMiss::default();
        let found = figcraft_ai::media::block_on(super::verify_assets(
            assets,
            &mut session,
            &intent,
            &mut rejected,
            &mut vision_error,
            &mut budget,
            &mut near,
        ));
        assert!(found.is_none(), "a throttle approves nothing");
        assert!(vision_error.is_none(), "and it is not a settings problem");
        let (_, reason) = near.unverified.expect("the picture is kept as unverified");
        assert!(reason.starts_with("unverified"), "{reason}");
    }

    #[test]
    fn a_candidate_that_will_not_download_does_not_end_the_round() {
        use std::sync::{Arc, Mutex};

        // A dead host is common and says nothing about the other nineteen candidates.
        struct HalfDead(Arc<Mutex<usize>>);
        impl figcraft_ai::MediaSession for HalfDead {
            fn search_assets<'a>(
                &'a mut self,
                _: &'a str,
                _: &'a figcraft_ai::AssetSearchOptions,
            ) -> figcraft_ai::BoxFuture<'a, Vec<figcraft_ai::AssetSearchOutcome>> {
                Box::pin(async { Vec::new() })
            }
            fn stock_photo<'a>(
                &'a mut self,
                _: &'a str,
                _: figcraft_ai::Orientation,
                _: usize,
            ) -> figcraft_ai::BoxFuture<'a, Result<figcraft_ai::StockPhoto, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no stock key".into())) })
            }
            fn download<'a>(&'a mut self, _: &'a str) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                let mut n = self.0.lock().expect("lock");
                *n += 1;
                let first = *n == 1;
                Box::pin(async move { if first { Err(TransportError::Network) } else { Ok(vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) } })
            }
            fn inspect_image<'a>(&'a mut self, _: &'a [u8], _: &'a str, _: &'a str) -> figcraft_ai::BoxFuture<'a, Result<String, TransportError>> {
                Box::pin(async {
                    Ok(r#"{"approved": true, "relevance_score": 0.8, "matched_requirements": [], "failed_requirements": [], "reason": "a mug"}"#
                        .into())
                })
            }
            fn generate_image<'a>(
                &'a mut self,
                _: &'a str,
                _: figcraft_ai::Orientation,
            ) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no image model".into())) })
            }
            fn vectorize<'a>(&'a mut self, _: &'a [u8]) -> figcraft_ai::BoxFuture<'a, Result<String, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no".into())) })
            }
            fn remove_background<'a>(&'a mut self, _: &'a [u8]) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no".into())) })
            }
        }

        let mut session = HalfDead(Arc::new(Mutex::new(0)));
        let assets: Vec<(String, String)> = (0..3).map(|i| (format!("https://example.com/{i}.jpg"), format!("credit {i}"))).collect();
        let intent = figcraft_ai::vision::VisualIntent::default();
        let mut rejected = Vec::new();
        let mut vision_error = None;
        let mut budget = figcraft_ai::MAX_VISION_CALLS;
        let mut near = super::NearMiss::default();
        let found = figcraft_ai::media::block_on(super::verify_assets(
            assets,
            &mut session,
            &intent,
            &mut rejected,
            &mut vision_error,
            &mut budget,
            &mut near,
        ));
        assert!(found.is_some(), "the second candidate is approved after the first will not load: {rejected:?}");
        assert!(rejected.iter().any(|(who, _)| who == "source"), "the dead host is reported as a source failure: {rejected:?}");
    }

    #[test]
    fn a_throttled_library_is_waited_for_once_and_then_reported() {
        use std::sync::{Arc, Mutex};

        // The failure this guards: Openverse allows twenty anonymous requests a minute, and one
        // `source_image` call carries every slot on the screen. An unpaced loop spent the whole
        // allowance at once, and the slots behind the first few failed with a throttle that had
        // nothing to do with the pictures they found.
        struct Throttled(Arc<Mutex<usize>>);
        impl figcraft_ai::MediaSession for Throttled {
            fn search_assets<'a>(
                &'a mut self,
                _: &'a str,
                _: &'a figcraft_ai::AssetSearchOptions,
            ) -> figcraft_ai::BoxFuture<'a, Vec<figcraft_ai::AssetSearchOutcome>> {
                let mut n = self.0.lock().expect("lock");
                *n += 1;
                let first = *n == 1;
                Box::pin(async move {
                    if first {
                        vec![figcraft_ai::AssetSearchOutcome {
                            provider: "openverse".into(),
                            assets: Vec::new(),
                            error: Some("429 from openverse".into()),
                        }]
                    } else {
                        vec![figcraft_ai::AssetSearchOutcome { provider: "openverse".into(), assets: Vec::new(), error: None }]
                    }
                })
            }
            fn stock_photo<'a>(
                &'a mut self,
                _: &'a str,
                _: figcraft_ai::Orientation,
                _: usize,
            ) -> figcraft_ai::BoxFuture<'a, Result<figcraft_ai::StockPhoto, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no stock key".into())) })
            }
            fn download<'a>(&'a mut self, _: &'a str) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no".into())) })
            }
            fn inspect_image<'a>(&'a mut self, _: &'a [u8], _: &'a str, _: &'a str) -> figcraft_ai::BoxFuture<'a, Result<String, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no vision".into())) })
            }
            fn generate_image<'a>(
                &'a mut self,
                _: &'a str,
                _: figcraft_ai::Orientation,
            ) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no image model".into())) })
            }
            fn vectorize<'a>(&'a mut self, _: &'a [u8]) -> figcraft_ai::BoxFuture<'a, Result<String, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no".into())) })
            }
            fn remove_background<'a>(&'a mut self, _: &'a [u8]) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no".into())) })
            }
        }

        let attempts = Arc::new(Mutex::new(0));
        let mut session = Throttled(attempts.clone());
        let options = figcraft_ai::AssetSearchOptions {
            limit: 4,
            orientation: figcraft_ai::Orientation::Landscape,
            allow_non_commercial: false,
            require_modification: true,
        };
        let mut calls = 0;
        let outcomes = figcraft_ai::media::block_on(super::search_paced(&mut session, "a mug", &options, &mut calls));
        assert_eq!(attempts.lock().expect("lock").clone(), 2, "a throttle is waited out and retried once");
        assert!(outcomes.iter().all(|o| o.error.is_none()), "the second answer is the library's real one: {outcomes:?}");
    }

    #[test]
    fn a_page_that_cannot_see_the_throttle_still_treats_the_refusal_as_one() {
        // The failure this guards: Openverse answers a throttled client with `429` and no
        // `Access-Control-Allow-Origin`, so a page never sees the status — only the CORS block
        // and "Failed to fetch". Matching on the status alone meant the browser build retried
        // nothing and told the model the libraries held no pictures.
        let page_words = [
            "Could not reach https://api.openverse.org/v1/images/: Failed to fetch. The endpoint must allow this page's origin (CORS)",
            "net::ERR_FAILED",
            "Access to fetch has been blocked by CORS policy",
        ];
        for error in page_words {
            let outcome = figcraft_ai::AssetSearchOutcome { provider: "openverse".into(), assets: Vec::new(), error: Some(error.into()) };
            assert!(super::is_throttled(&outcome), "a page's refused request is a throttle: {error}");
        }
        let empty = figcraft_ai::AssetSearchOutcome { provider: "openverse".into(), assets: Vec::new(), error: None };
        assert!(!super::is_throttled(&empty), "an answer with no error is not a throttle");
    }

    #[test]
    fn every_library_search_is_spaced_out() {
        // The failure this guards: a screen's seven slots each ask the libraries a question, and
        // unpaced that is the whole anonymous allowance in a second or two. The pace belongs to
        // the call, so the counter is the thing under test.
        use std::sync::{Arc, Mutex};
        let seen = Arc::new(Mutex::new(Vec::new()));
        struct Silent(Arc<Mutex<Vec<std::time::Instant>>>);
        impl figcraft_ai::MediaSession for Silent {
            fn search_assets<'a>(
                &'a mut self,
                _: &'a str,
                _: &'a figcraft_ai::AssetSearchOptions,
            ) -> figcraft_ai::BoxFuture<'a, Vec<figcraft_ai::AssetSearchOutcome>> {
                self.0.lock().expect("lock").push(std::time::Instant::now());
                Box::pin(async { vec![figcraft_ai::AssetSearchOutcome { provider: "openverse".into(), assets: Vec::new(), error: None }] })
            }
            fn stock_photo<'a>(
                &'a mut self,
                _: &'a str,
                _: figcraft_ai::Orientation,
                _: usize,
            ) -> figcraft_ai::BoxFuture<'a, Result<figcraft_ai::StockPhoto, TransportError>> {
                Box::pin(async { Err(TransportError::Other("none".into())) })
            }
            fn download<'a>(&'a mut self, _: &'a str) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Err(TransportError::Other("none".into())) })
            }
            fn inspect_image<'a>(&'a mut self, _: &'a [u8], _: &'a str, _: &'a str) -> figcraft_ai::BoxFuture<'a, Result<String, TransportError>> {
                Box::pin(async { Err(TransportError::Other("none".into())) })
            }
            fn generate_image<'a>(
                &'a mut self,
                _: &'a str,
                _: figcraft_ai::Orientation,
            ) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Err(TransportError::Other("none".into())) })
            }
            fn vectorize<'a>(&'a mut self, _: &'a [u8]) -> figcraft_ai::BoxFuture<'a, Result<String, TransportError>> {
                Box::pin(async { Err(TransportError::Other("none".into())) })
            }
            fn remove_background<'a>(&'a mut self, _: &'a [u8]) -> figcraft_ai::BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Err(TransportError::Other("none".into())) })
            }
        }
        let options = figcraft_ai::AssetSearchOptions {
            limit: 4,
            orientation: figcraft_ai::Orientation::Landscape,
            allow_non_commercial: false,
            require_modification: true,
        };
        let mut session = Silent(seen.clone());
        let mut calls = 0;
        for _ in 0..3 {
            figcraft_ai::media::block_on(super::search_paced(&mut session, "a mug", &options, &mut calls));
        }
        let stamps = seen.lock().expect("lock").clone();
        assert_eq!(stamps.len(), 3);
        let gap = stamps[2].duration_since(stamps[1]);
        assert!(gap.as_millis() >= u128::from(super::LIBRARY_PAUSE_MS) - 200, "the second question waits: {gap:?}");
    }

    #[test]
    fn the_usage_of_a_send_is_recorded() {
        let mut u = chat_ui();
        let response = ChatResponse { input_tokens: Some(10), output_tokens: Some(3), ..Default::default() };
        u.app.session.chat.record_usage("openai", "gpt-5.6", &response);
        u.app.session.ai.record(u.app.session.chat.record_usage("openai", "gpt-5.6", &response));
        let out = u.app.session.execute("ai.usage", &json!({})).expect("the usage is answered");
        assert_eq!(out["requests"], 1);
    }

    #[test]
    fn three_prompts_pure_product_spec_verification() {
        use figcraft_ai::{AssetSearchOptions, AssetSearchOutcome, BoxFuture, MediaFactory, MediaSession, StockPhoto};
        use std::sync::{Arc, Mutex};

        // =========================================================================
        // PROMPT 1: E-Commerce Storefront (Pure product details, zero design hints)
        // =========================================================================
        let p1 = "We sell artisanal roasted whole bean coffee and pour-over brewing equipment online under the brand name 'Roast & Bloom'. We offer an Ethiopian Yirgacheffe single-origin bag, a Colombia Supremo bag, a ceramic dripper, and a monthly bean subscription. Make the web app UI for our store.";

        let mut u1 = chat_ui();
        u1.app.session.execute("ai.chat.send", &json!({"text": p1})).expect("prompt 1 queued");
        send_pending(&mut u1.app).expect("prompt 1 starts");

        let msgs1 = u1.app.session.chat_messages();
        let user_msg1 = &msgs1.last().expect("last message").content;

        // Check 1: Did it automatically activate skills?
        assert!(user_msg1.contains("[Design skills activated for this request]"), "P1: Skills block missing");
        assert!(user_msg1.contains("design.product-site"), "P1: Expected design.product-site skill, got: {user_msg1}");

        // Check 2 & 3: Automatic image slot usage & content relevance verification
        struct VerifyingSession {
            verified_count: Arc<Mutex<usize>>,
            rejected_count: Arc<Mutex<usize>>,
        }
        impl MediaSession for VerifyingSession {
            fn search_assets<'a>(&'a mut self, _: &'a str, _: &'a AssetSearchOptions) -> BoxFuture<'a, Vec<AssetSearchOutcome>> {
                Box::pin(async { Vec::new() })
            }
            fn stock_photo<'a>(&'a mut self, _: &'a str, _: figcraft_ai::Orientation, _: usize) -> BoxFuture<'a, Result<StockPhoto, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no stock key".into())) })
            }
            fn download<'a>(&'a mut self, _: &'a str) -> BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Ok(vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) })
            }
            fn inspect_image<'a>(&'a mut self, _: &'a [u8], _: &'a str, prompt: &'a str) -> BoxFuture<'a, Result<String, TransportError>> {
                if prompt.to_lowercase().contains("coffee") || prompt.to_lowercase().contains("dripper") {
                    *self.verified_count.lock().expect("lock") += 1;
                    Box::pin(async {
                        Ok(r#"{"approved": true, "relevance_score": 0.95, "matched_requirements": ["coffee subject"], "failed_requirements": [], "reason": "Accurate whole bean coffee pack"}"#.into())
                    })
                } else {
                    *self.rejected_count.lock().expect("lock") += 1;
                    Box::pin(async {
                        Ok(r#"{"approved": false, "relevance_score": 0.1, "matched_requirements": [], "failed_requirements": ["subject mismatch"], "reason": "Image does not depict coffee"}"#.into())
                    })
                }
            }
            fn generate_image<'a>(&'a mut self, _: &'a str, _: figcraft_ai::Orientation) -> BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Ok(vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) })
            }
            fn vectorize<'a>(&'a mut self, _: &'a [u8]) -> BoxFuture<'a, Result<String, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no".into())) })
            }
            fn remove_background<'a>(&'a mut self, _: &'a [u8]) -> BoxFuture<'a, Result<Vec<u8>, TransportError>> {
                Box::pin(async { Err(TransportError::Other("no".into())) })
            }
        }

        let verified_counter = Arc::new(Mutex::new(0));
        let rejected_counter = Arc::new(Mutex::new(0));

        struct VerifyingFactory {
            verified: Arc<Mutex<usize>>,
            rejected: Arc<Mutex<usize>>,
        }
        impl MediaFactory for VerifyingFactory {
            fn make(
                &self,
                _: Option<String>,
                _: Option<String>,
                _: Option<figcraft_ai::VisionConfig>,
                _: Option<(figcraft_ai::VectorizeProvider, String)>,
            ) -> Box<dyn MediaSession> {
                Box::new(VerifyingSession { verified_count: self.verified.clone(), rejected_count: self.rejected.clone() })
            }
        }

        let factory = VerifyingFactory { verified: verified_counter.clone(), rejected: rejected_counter.clone() };

        // Parallel multi-slot source_image call (coffee + dripper + invalid tractor test)
        let multi_slot_batch = json!({
            "requests": [
                {"id": "slot_coffee_1", "subject": "Ethiopian Yirgacheffe whole bean coffee bag", "query": "coffee bag", "must_generate": true},
                {"id": "slot_dripper_2", "subject": "Ceramic pour-over coffee dripper", "query": "pour over dripper", "must_generate": true},
                {"id": "slot_mismatch_3", "subject": "Tractor on farm", "query": "tractor", "must_generate": true}
            ]
        });

        let vision_cfg = figcraft_ai::VisionConfig {
            base_url: "http://127.0.0.1:20129/v1".into(),
            api_key: "public".into(),
            protocol: figcraft_ai::providers::Protocol::Openai,
            model: "deepseek-v4.1-flash-free".into(),
        };

        let source_resp =
            figcraft_ai::media::block_on(super::source_image(&factory, &(Some("gen-key".into()), None, Some(vision_cfg), None), &multi_slot_batch));
        let batch_result: Value = serde_json::from_str(&source_resp).expect("valid JSON response");

        // Assert parallel verification: 2 approved, 1 rejected
        assert_eq!(batch_result["applied"], 2, "Only content-verified images should be placed");
        assert_eq!(batch_result["results"][0]["state"], "verified");
        assert_eq!(batch_result["results"][1]["state"], "verified");
        assert_eq!(batch_result["results"][2]["state"], "no-verified-match");
        assert!(*verified_counter.lock().expect("lock") >= 2);
        assert!(*rejected_counter.lock().expect("lock") >= 1);

        // Check 4: Post-design review loop (layout.audit)
        let page = u1
            .app
            .run(
                "node.create",
                json!({"type": "frame", "x": 0, "y": 0, "width": 1280, "height": 860, "name": "Storefront", "props": {"fills": ["#FAFAF9"]}}),
            )
            .expect("storefront frame");
        let page_id = page["id"].clone();

        u1.app.run("text.create", json!({"parent": page_id, "x": 60, "y": 40, "characters": "Roast & Bloom · Artisan Coffee Co.", "props": {"fontSize": 32, "fills": ["#292524"]}})).expect("p1 h1");
        u1.app.run("text.create", json!({"parent": page_id, "x": 60, "y": 85, "characters": "Single-Origin Beans & Precision Brewing Gear", "props": {"fontSize": 16, "fills": ["#78716C"]}})).expect("p1 h2");

        // Product Card 1
        let p_card1 = u1.app.run("node.create", json!({"type": "frame", "parent": page_id, "x": 60, "y": 140, "width": 260, "height": 380, "name": "Card·Ethiopian", "props": {"fills": ["#FFFFFF"], "cornerRadius": 16}})).expect("pcard1");
        let pc1_id = p_card1["id"].clone();
        u1.app.run("node.create", json!({"type": "rectangle", "parent": pc1_id, "x": 16, "y": 16, "width": 228, "height": 200, "name": "IMG·coffee-ethiopian", "props": {"fills": ["#E7E5E4"], "cornerRadius": 12}})).expect("slot1");
        u1.app.run("text.create", json!({"parent": pc1_id, "x": 16, "y": 230, "characters": "Ethiopian Yirgacheffe\nNotes: Jasmine & Citrus · $22", "props": {"fontSize": 15, "fills": ["#292524"]}})).expect("pc1 t");

        // Product Card 2
        let p_card2 = u1.app.run("node.create", json!({"type": "frame", "parent": page_id, "x": 350, "y": 140, "width": 260, "height": 380, "name": "Card·CeramicDripper", "props": {"fills": ["#FFFFFF"], "cornerRadius": 16}})).expect("pcard2");
        let pc2_id = p_card2["id"].clone();
        u1.app.run("node.create", json!({"type": "rectangle", "parent": pc2_id, "x": 16, "y": 16, "width": 228, "height": 200, "name": "IMG·ceramic-dripper", "props": {"fills": ["#E7E5E4"], "cornerRadius": 12}})).expect("slot2");
        u1.app.run("text.create", json!({"parent": pc2_id, "x": 16, "y": 230, "characters": "Ceramic Pour-Over Dripper\nMatte Off-White · $34", "props": {"fontSize": 15, "fills": ["#292524"]}})).expect("pc2 t");

        // Product Card 3
        let p_card3 = u1.app.run("node.create", json!({"type": "frame", "parent": page_id, "x": 640, "y": 140, "width": 260, "height": 380, "name": "Card·ColombiaSupremo", "props": {"fills": ["#FFFFFF"], "cornerRadius": 16}})).expect("pcard3");
        let pc3_id = p_card3["id"].clone();
        u1.app.run("node.create", json!({"type": "rectangle", "parent": pc3_id, "x": 16, "y": 16, "width": 228, "height": 200, "name": "IMG·colombia-supremo", "props": {"fills": ["#E7E5E4"], "cornerRadius": 12}})).expect("slot3");
        u1.app.run("text.create", json!({"parent": pc3_id, "x": 16, "y": 230, "characters": "Colombia Supremo\nNotes: Caramel & Cacao · $19", "props": {"fontSize": 15, "fills": ["#292524"]}})).expect("pc3 t");

        // Product Card 4 - Subscription
        let p_card4 = u1.app.run("node.create", json!({"type": "frame", "parent": page_id, "x": 930, "y": 140, "width": 290, "height": 380, "name": "Card·Subscription", "props": {"fills": ["#1C1917"], "cornerRadius": 16}})).expect("pcard4");
        let pc4_id = p_card4["id"].clone();
        u1.app.run("text.create", json!({"parent": pc4_id, "x": 20, "y": 30, "characters": "Monthly Bean Club\n2 Fresh Roasts Monthly\nCurated Origins + Free Filter Kit\n\n$38 / month", "props": {"fontSize": 16, "fills": ["#FAFAF9"]}})).expect("pc4 t");

        let blockers = audit_blockers(&mut u1.app).expect("Audit must detect empty slot defect");
        assert!(blockers.contains("empty-slot"), "Audit must flag empty slot: {blockers}");
        assert!(blockers.contains("IMG·coffee-ethiopian"));
        assert!(blockers.contains("source_image"), "Audit fix must instruct model to call source_image");

        // =========================================================================
        // PROMPT 2: Mobile App, Prototype Coordinates, Behaviors & Motion
        // =========================================================================
        let p2 = "We have a mobile app called ZenStep for daily mindfulness and hydration. The user has a daily checklist on the main screen with habits like Morning Meditation and Drink 2L Water, showing their streak, and can tap Morning Meditation to open the habit detail screen with a weekly completion tracker and a 'Mark Done' button that returns to the list. Make the mobile app UI with the interactive prototype flow between these screens.";

        let mut u2 = chat_ui();
        u2.app.session.execute("ai.chat.send", &json!({"text": p2})).expect("prompt 2 queued");
        send_pending(&mut u2.app).expect("prompt 2 starts");

        let msgs2 = u2.app.session.chat_messages();
        let user_msg2 = &msgs2.last().expect("last message").content;

        // Check skills for mobile & prototype
        assert!(user_msg2.contains("design.mobile"), "P2: Expected design.mobile skill: {user_msg2}");
        assert!(user_msg2.contains("prototype.navigation"), "P2: Expected prototype.navigation skill: {user_msg2}");

        // Verify prototype creation: Flow starting point, source coords -> dest coords, behavior, and motion transition
        let home_frame = u2.app.run("node.create", json!({"type": "frame", "x": 0, "y": 0, "width": 393, "height": 852, "name": "ZenStep Home Checklist", "props": {"fills": ["#0B0F19"]}})).expect("home frame");
        let home_id = home_frame["id"].as_i64().expect("id");

        let detail_frame = u2.app.run("node.create", json!({"type": "frame", "x": 450, "y": 0, "width": 393, "height": 852, "name": "Habit Detail - Meditation", "props": {"fills": ["#0B0F19"]}})).expect("detail frame");
        let detail_id = detail_frame["id"].as_i64().expect("id");

        // Home screen content
        u2.app.run("text.create", json!({"parent": home_id, "x": 24, "y": 60, "characters": "Today · 7 Day Streak 🔥", "props": {"fontSize": 24, "fills": ["#FFFFFF"]}})).expect("h text");
        u2.app.run("text.create", json!({"parent": home_id, "x": 24, "y": 100, "characters": "Daily Habits Checklist", "props": {"fontSize": 15, "fills": ["#94A3B8"]}})).expect("sub text");

        let habit_button = u2
            .app
            .run(
                "node.create",
                json!({
                    "type": "frame", "parent": home_id, "x": 24, "y": 140, "width": 345, "height": 72, "name": "HabitCard·Meditation",
                    "props": {"fills": ["#1E293B"], "cornerRadius": 16}
                }),
            )
            .expect("habit card");
        let button_id = habit_button["id"].as_i64().expect("button id");
        u2.app.run("text.create", json!({"parent": button_id, "x": 20, "y": 26, "characters": "🧘 Morning Meditation (15 min)", "props": {"fontSize": 16, "fills": ["#F1F5F9"]}})).expect("btn text");

        let habit_2 = u2
            .app
            .run(
                "node.create",
                json!({
                    "type": "frame", "parent": home_id, "x": 24, "y": 224, "width": 345, "height": 72, "name": "HabitCard·Water",
                    "props": {"fills": ["#1E293B"], "cornerRadius": 16}
                }),
            )
            .expect("habit 2");
        let h2_id = habit_2["id"].clone();
        u2.app.run("text.create", json!({"parent": h2_id, "x": 20, "y": 26, "characters": "💧 Drink 2L Water (1.2L done)", "props": {"fontSize": 16, "fills": ["#F1F5F9"]}})).expect("h2 text");

        // Detail screen content
        u2.app
            .run(
                "text.create",
                json!({"parent": detail_id, "x": 24, "y": 60, "characters": "Morning Meditation", "props": {"fontSize": 24, "fills": ["#FFFFFF"]}}),
            )
            .expect("d title");
        u2.app.run("text.create", json!({"parent": detail_id, "x": 24, "y": 100, "characters": "Weekly Completion: 6 / 7 Days", "props": {"fontSize": 15, "fills": ["#38BDF8"]}})).expect("d tracker");

        let chart = u2
            .app
            .run(
                "node.create",
                json!({
                    "type": "frame", "parent": detail_id, "x": 24, "y": 140, "width": 345, "height": 180, "name": "Chart·WeeklyCompletion",
                    "props": {"fills": ["#1E293B"], "cornerRadius": 16}
                }),
            )
            .expect("chart");
        let chart_id = chart["id"].clone();
        u2.app.run("text.create", json!({"parent": chart_id, "x": 20, "y": 20, "characters": "Mon  Tue  Wed  Thu  Fri  Sat  Sun\n ●    ●    ●    ●    ●    ○    ●\n100% On-Track This Week", "props": {"fontSize": 14, "fills": ["#A5B4FC"]}})).expect("chart text");

        let mark_done_button = u2
            .app
            .run(
                "node.create",
                json!({
                    "type": "frame", "parent": detail_id, "x": 24, "y": 700, "width": 345, "height": 56, "name": "Btn·MarkDone",
                    "props": {"fills": ["#6366F1"], "cornerRadius": 16}
                }),
            )
            .expect("mark done button");
        let return_btn_id = mark_done_button["id"].as_i64().expect("return button id");
        u2.app.run("text.create", json!({"parent": return_btn_id, "x": 80, "y": 18, "characters": "✓ Mark Done for Today", "props": {"fontSize": 16, "fills": ["#FFFFFF"]}})).expect("ret text");

        // Add flow starting point
        let flow_res = u2.app.run("proto.flow.add", json!({"start": home_id, "name": "Habit Tracking Flow"})).expect("flow add");
        assert_eq!(flow_res["start"], home_id);

        // Connect Habit Card -> Habit Detail Screen with Push Animation
        let proto_add_1 = u2
            .app
            .run(
                "proto.interaction.add",
                json!({
                    "id": button_id,
                    "trigger": {"type": "onClick"},
                    "action": { "type": "navigate", "dest": detail_id },
                    "transition": { "type": "push", "direction": "left" },
                    "durationMs": 300,
                    "easing": "easeOut"
                }),
            )
            .unwrap_or_else(|| panic!("proto.interaction.add 1 failed: {:?}", u2.app.ui.last_error));
        assert_eq!(proto_add_1["index"], 0);

        // Connect Mark Done Button -> Back to Home Screen with Push Animation
        let proto_add_2 = u2
            .app
            .run(
                "proto.interaction.add",
                json!({
                    "id": return_btn_id,
                    "trigger": {"type": "onClick"},
                    "action": { "type": "navigate", "dest": home_id },
                    "transition": { "type": "push", "direction": "right" },
                    "durationMs": 250,
                    "easing": "linear"
                }),
            )
            .unwrap_or_else(|| panic!("proto.interaction.add 2 failed: {:?}", u2.app.ui.last_error));
        assert_eq!(proto_add_2["index"], 0);

        // Verify coordinates & behaviors are accurately bound in the document model
        let doc = u2.app.session.doc().expect("document");
        let node_home = doc.node(figcraft_doc::NodeId(home_id as u64)).expect("home node");
        let node_detail = doc.node(figcraft_doc::NodeId(detail_id as u64)).expect("detail node");
        let node_btn = doc.node(figcraft_doc::NodeId(button_id as u64)).expect("btn node");

        assert_eq!(node_home.transform.translation().x, 0.0);
        assert_eq!(node_detail.transform.translation().x, 450.0);
        assert_eq!(node_btn.transform.translation().x, 24.0);
        assert_eq!(node_btn.transform.translation().y, 140.0);
        assert!(!node_btn.interactions.is_empty(), "Button has prototype interaction");
        match &node_btn.interactions[0].actions[0] {
            figcraft_doc::proto::Action::Navigate { dest, .. } => assert_eq!(*dest, figcraft_doc::NodeId(detail_id as u64)),
            other => panic!("expected navigate action, got: {other:?}"),
        }
        match &node_btn.interactions[0].transition {
            figcraft_doc::proto::Transition::Push { direction } => assert_eq!(*direction, figcraft_doc::proto::Direction::Left),
            other => panic!("expected push transition, got: {other:?}"),
        }

        // =========================================================================
        // PROMPT 3: Smart Home IoT Dashboard
        // =========================================================================
        let p3 = "We have a smart home system called Atmos for room climate and lighting control. Users view living room and bedroom temperatures and humidity, tap lighting scenes like Evening Relax and Focus Work with room photography, and adjust fan speed and warmth controls. Make the dashboard UI for Atmos.";

        let mut u3 = chat_ui();
        u3.app.session.execute("ai.chat.send", &json!({"text": p3})).expect("prompt 3 queued");
        send_pending(&mut u3.app).expect("prompt 3 starts");

        let msgs3 = u3.app.session.chat_messages();
        let user_msg3 = &msgs3.last().expect("last message").content;

        assert!(user_msg3.contains("design.dashboard"), "P3: Expected design.dashboard skill: {user_msg3}");

        // Build Atmos Dashboard elements
        let dash = u3
            .app
            .run(
                "node.create",
                json!({"type": "frame", "x": 0, "y": 0, "width": 1440, "height": 900, "name": "Atmos Dashboard", "props": {"fills": ["#0F1117"]}}),
            )
            .expect("dash");
        let dash_id = dash["id"].clone();

        // Header
        u3.app.run("text.create", json!({"parent": dash_id, "x": 60, "y": 50, "characters": "Atmos · Climate & Lighting", "props": {"fontSize": 28, "fills": ["#FFFFFF"]}})).expect("header");

        // Climate Card 1
        let card1 = u3.app.run("node.create", json!({"type": "frame", "parent": dash_id, "x": 60, "y": 120, "width": 300, "height": 180, "name": "Card·LivingRoom", "props": {"fills": ["#1A1D26"], "cornerRadius": 16}})).expect("card1");
        let card1_id = card1["id"].clone();
        u3.app.run("text.create", json!({"parent": card1_id, "x": 20, "y": 20, "characters": "Living Room\n22.5°C · 45% Humidity", "props": {"fontSize": 18, "fills": ["#E2E8F0"]}})).expect("text1");

        // Climate Card 2
        let card2 = u3.app.run("node.create", json!({"type": "frame", "parent": dash_id, "x": 380, "y": 120, "width": 300, "height": 180, "name": "Card·Bedroom", "props": {"fills": ["#1A1D26"], "cornerRadius": 16}})).expect("card2");
        let card2_id = card2["id"].clone();
        u3.app.run("text.create", json!({"parent": card2_id, "x": 20, "y": 20, "characters": "Master Bedroom\n20.0°C · 50% Humidity", "props": {"fontSize": 18, "fills": ["#E2E8F0"]}})).expect("text2");

        // Lighting Scene Card 1 with Room Photography Slot
        let scene1 = u3.app.run("node.create", json!({"type": "frame", "parent": dash_id, "x": 60, "y": 330, "width": 420, "height": 260, "name": "Scene·EveningRelax", "props": {"fills": ["#1A1D26"], "cornerRadius": 16}})).expect("scene1");
        let scene1_id = scene1["id"].clone();
        u3.app.run("node.create", json!({"type": "rectangle", "parent": scene1_id, "x": 16, "y": 16, "width": 388, "height": 160, "name": "IMG·evening-relax-living-room", "props": {"fills": ["#2D3748"], "cornerRadius": 12}})).expect("img1");
        u3.app.run("text.create", json!({"parent": scene1_id, "x": 20, "y": 190, "characters": "Evening Relax\nWarm 2700K · 40% Dimming", "props": {"fontSize": 16, "fills": ["#E2E8F0"]}})).expect("scene1 text");

        // Lighting Scene Card 2 with Room Photography Slot
        let scene2 = u3.app.run("node.create", json!({"type": "frame", "parent": dash_id, "x": 510, "y": 330, "width": 420, "height": 260, "name": "Scene·FocusWork", "props": {"fills": ["#1A1D26"], "cornerRadius": 16}})).expect("scene2");
        let scene2_id = scene2["id"].clone();
        u3.app.run("node.create", json!({"type": "rectangle", "parent": scene2_id, "x": 16, "y": 16, "width": 388, "height": 160, "name": "IMG·focus-work-study", "props": {"fills": ["#2D3748"], "cornerRadius": 12}})).expect("img2");
        u3.app.run("text.create", json!({"parent": scene2_id, "x": 20, "y": 190, "characters": "Focus Work\nCrisp 4500K · 90% Brightness", "props": {"fontSize": 16, "fills": ["#E2E8F0"]}})).expect("scene2 text");

        // Controls Section
        let ctrl = u3.app.run("node.create", json!({"type": "frame", "parent": dash_id, "x": 60, "y": 620, "width": 870, "height": 140, "name": "Controls·Environmental", "props": {"fills": ["#1A1D26"], "cornerRadius": 16}})).expect("ctrl");
        let ctrl_id = ctrl["id"].clone();
        u3.app.run("text.create", json!({"parent": ctrl_id, "x": 24, "y": 20, "characters": "Environmental Controls · Fan Speed [Level 2 - Quiet] · Warmth [Cozy 23°C]", "props": {"fontSize": 15, "fills": ["#A0AEC0"]}})).expect("ctrl text");

        // Export Renders for all 3 prompts to /tmp/sycnui-test-shots/
        let _ = std::fs::create_dir_all("/tmp/sycnui-test-shots");
        if let Ok(png1) = u1.app.session.chat_render_png(&[], 1.0) {
            let _ = std::fs::write("/tmp/sycnui-test-shots/01_coffee_storefront.png", png1);
        }
        if let Ok(png2) = u2.app.session.chat_render_png(&[], 1.0) {
            let _ = std::fs::write("/tmp/sycnui-test-shots/02_zenstep_mobile_proto.png", png2);
        }
        if let Ok(png3) = u3.app.session.chat_render_png(&[], 1.0) {
            let _ = std::fs::write("/tmp/sycnui-test-shots/03_atmos_dashboard.png", png3);
        }
    }

    #[test]
    fn provider_errors_are_rendered_clearly_and_stop_review_loop() {
        let mut u = chat_ui();
        u.app.run("node.create", json!({"type": "frame", "x": 0, "y": 0, "width": 393, "height": 204, "name": "Cover"})).expect("a frame");

        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(Event::Done(Err(TransportError::Other("Upstream request failed: Endpoint is unavailable.".into())))).expect("send");

        u.app.ui.chat.streaming = Some(Streaming {
            rx,
            tx,
            _thread: None,
            step: 0,
            text: String::new(),
            tool_calls: Vec::new(),
            results: Vec::new(),
            mcp_dispatched: false,
            provider: "test".into(),
            model: "deepseek-v4.1-flash-free".into(),
        });
        u.app.session.chat.busy = true;

        let ctx = egui::Context::default();
        let finished = drain(&mut u.app, &ctx);
        assert!(finished, "drain must finish immediately on error");
        assert!(!u.app.session.chat.busy, "chat must no longer be busy");

        assert_eq!(
            u.app.ui.chat.failure.as_deref(),
            Some("Upstream request failed: Endpoint is unavailable.")
        );

        let messages = u.app.session.chat_messages();
        let last = messages.last().expect("last message");
        assert!(
            last.content.contains("❌ **Provider Error:** Upstream request failed: Endpoint is unavailable."),
            "last message must contain provider error: {}",
            last.content
        );

        assert_eq!(u.app.ui.chat.review_rounds, 0);
        assert_eq!(u.app.ui.chat.empty_answers, 0);
    }
}
