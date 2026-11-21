//! The layout audit: the faults that are geometry, not taste.
//!
//! A designer looks at a screen and sees a headline running past its column, a card whose text
//! spills below it, a layer left loose on the page. The Vision model sees those too, sometimes,
//! but it sees a 1440-pixel screen scaled into a thumbnail and it answers in prose — it cannot
//! say `id 82 is 701 px of text in a 566 px column`. That answer is arithmetic, and arithmetic
//! belongs in a command the model runs rather than a guess it makes.
//!
//! The audit reads the document and reports nothing it did not measure: every finding carries the
//! layer id, the numbers, and the fix. It never changes anything.

use serde_json::{Value, json};

use super::CommandSpec;
use crate::{Result, Session, spec};

/// How much of a clip a finding tolerates before it is reported. A glyph's ink can sit a fraction
/// of a pixel past its box in a tight line height; a word running 135 px past its column cannot.
const TOLERANCE: f64 = 1.0;

/// The kinds whose text can be clipped by a fixed box.
fn is_text(n: &figcraft_doc::Node) -> bool {
    matches!(n.kind, figcraft_doc::NodeKind::Text(_))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![spec!(
        "layout.audit",
        "Audit the layout",
        "ids?: array of int — the frames to audit, default the page's top-level frames; depth?: int — how deep to walk (default 32). Reports clipped text, text wider than its fixed box, layers outside their frame, picture slots with no picture in them, empty frames and frames with no size. Reads only; changes nothing",
        false,
        run
    )]
}

fn run(s: &mut Session, params: &Value) -> Result<Value> {
    let ds = s.ds()?;
    let doc = &ds.doc;
    let roots: Vec<figcraft_doc::NodeId> = match params.get("ids").and_then(Value::as_array) {
        Some(list) => list.iter().filter_map(crate::params::id_of).collect(),
        None => doc.node(ds.page).map(|p| p.children.clone()).unwrap_or_default(),
    };
    let depth = params.get("depth").and_then(Value::as_u64).unwrap_or(32).min(256) as usize;
    // Only the page's own children can be "loose on the page"; a frame asked for by id was named
    // by the caller on purpose.
    let page_roots = params.get("ids").is_none();
    let mut findings = Vec::new();
    for root in roots {
        audit_node(doc, root, depth, page_roots, &mut findings);
    }
    let clipped = findings.iter().filter(|f| f["kind"] == "text-clipped").count();
    let stray = findings.iter().filter(|f| f["kind"] == "stray-layer").count();
    let empty_slots = findings.iter().filter(|f| f["kind"] == "empty-slot").count();
    let empty_containers = findings.iter().filter(|f| f["kind"] == "empty-container").count();
    let button_overflow = findings.iter().filter(|f| f["kind"] == "button-overflow").count();
    let child_overflow = findings.iter().filter(|f| f["kind"] == "overflowing-child").count();
    let missing_images = findings.iter().filter(|f| f["kind"] == "missing-image").count();
    Ok(json!({
        "findings": findings,
        "counts": {
            "total": findings.len(),
            "textClipped": clipped,
            "strayLayer": stray,
            "emptySlot": empty_slots,
            "emptyContainer": empty_containers,
            "buttonOverflow": button_overflow,
            "childOverflow": child_overflow,
            "missingImage": missing_images,
        },
    }))
}

/// Walk one subtree, reporting faults on the way down.
fn audit_node(doc: &figcraft_doc::Document, id: figcraft_doc::NodeId, depth: usize, top: bool, out: &mut Vec<Value>) {
    let Some(n) = doc.node(id) else { return };
    if !n.visible {
        return;
    }
    audit_own(doc, n, out);
    audit_container(doc, n, out);
    if top && is_loose_shape(n) {
        out.push(json!({
            "kind": "stray-layer", "id": n.id.0, "name": n.name,
            "detail": format!(
                "a {} sits loose on the page at ({:.0}, {:.0}) outside every frame",
                kind_name(n),
                figcraft_geom::translation(n.transform).x,
                figcraft_geom::translation(n.transform).y
            ),
            "fix": "move it inside the frame it belongs to (node.reparent), or delete it",
        }));
    }
    if depth == 0 {
        return;
    }
    for child in &n.children {
        audit_node(doc, *child, depth - 1, false, out);
    }
}

/// Whether a node is a bare shape rather than a container. A shape that is a direct child of the
/// page is a layer the design forgot to put anywhere.
fn is_loose_shape(n: &figcraft_doc::Node) -> bool {
    use figcraft_doc::NodeKind as K;
    matches!(n.kind, K::Rectangle | K::Ellipse { .. } | K::Polygon { .. } | K::Star { .. } | K::Line | K::Vector { .. } | K::Text(_) | K::Slice)
}

/// The type name a person would say.
fn kind_name(n: &figcraft_doc::Node) -> String {
    use figcraft_doc::NodeKind as K;
    let s = match n.kind {
        K::Rectangle => "rectangle",
        K::Ellipse { .. } => "ellipse",
        K::Polygon { .. } => "polygon",
        K::Star { .. } => "star",
        K::Line => "line",
        K::Vector { .. } => "vector",
        K::Text(_) => "text layer",
        K::Slice => "slice",
        _ => "layer",
    };
    s.to_string()
}

/// The faults a node shows by itself.
fn audit_own(doc: &figcraft_doc::Document, n: &figcraft_doc::Node, out: &mut Vec<Value>) {
    let (w, h) = n.size();
    let name = &n.name;
    let id = n.id.0;

    if !w.is_finite() || !h.is_finite() || w < 0.0 || h < 0.0 {
        out.push(json!({
            "kind": "bad-size", "id": id, "name": name,
            "detail": format!("the layer measures {w} x {h}"),
            "fix": "set a real width and height",
        }));
    }

    if is_text(n)
        && let Some(fault) = text_fault(doc, n)
    {
        out.push(fault);
    }

    // A picture slot: a shape filled with an image, or one whose name marks it as media. Image
    // paints are fills like any other; a slot whose fill never arrived is a solid grey box.
    //
    // Only a shape can be a slot. A text layer whose copy happens to contain the word
    // "photography" is a headline, not a missing picture, and a frame with children in it is a
    // container. Reporting either is the audit crying wolf, and a model told to fill a headline
    // with photography spends a step on a tool that cannot help it.
    let can_be_slot = !is_text(n) && n.children.is_empty();
    // A frame named as a picture is one defect, not two: it is a slot waiting for its photograph,
    // and calling it an empty container as well told the model to fill a container it had already
    // filled with a picture slot, and to fill the slot it had already been told about. Twice the
    // findings for one grey box is half the model's fixing budget spent on nothing.
    let is_slot = can_be_slot && (names_a_picture(n) || has_image_paint(n)) && w > 40.0 && h > 40.0;
    if is_slot && !has_image_paint(n) {
        out.push(json!({
            "kind": "empty-slot", "id": id, "name": name,
            "detail": format!("a {} x {} box named as a picture with no image in it", w.round(), h.round()),
            "fix": "call source_image with this layer's id and a subject, or rename the layer if it is not a picture",
        }));
    }

    // A container with room for something and nothing in it. A section a model laid out but never
    // filled reads as a blank stripe on the page, which is the fault this catches.
    let empty_container = !is_slot
        && matches!(n.kind, figcraft_doc::NodeKind::Frame | figcraft_doc::NodeKind::Group | figcraft_doc::NodeKind::Section { .. })
        && n.children.is_empty()
        && w > 40.0
        && h > 40.0;
    if empty_container {
        out.push(json!({
            "kind": "empty-container", "id": id, "name": name,
            "detail": format!("a {} x {} frame with nothing in it", w.round(), h.round()),
            "fix": "fill it with the section it was named for, or delete it",
        }));
    }
}

/// The faults a container shows relative to its children: overflowing children, broken button text,
/// or missing photography in cards and collection tiles.
fn audit_container(doc: &figcraft_doc::Document, n: &figcraft_doc::Node, out: &mut Vec<Value>) {
    let (parent_w, parent_h) = n.size();
    if parent_w <= 0.0 || parent_h <= 0.0 {
        return;
    }

    let is_btn = is_button(n);
    let is_card = is_card_or_tile(n);
    let has_autolayout = n.layout.is_some();
    let mut button_overflow_reported = false;

    // Check if children overflow this container
    for child_id in &n.children {
        let Some(c) = doc.node(*child_id) else { continue };
        if !c.visible {
            continue;
        }
        let (c_w, c_h) = c.size();
        let trans = figcraft_geom::translation(c.transform);
        let c_x = trans.x;
        let c_y = trans.y;

        if c_w > 0.0 && c_h > 0.0 {
            let overflow_right = (c_x + c_w) - parent_w;
            let overflow_bottom = (c_y + c_h) - parent_h;
            let overflow_left = -c_x;
            let overflow_top = -c_y;

            if overflow_right > TOLERANCE || overflow_bottom > TOLERANCE || overflow_left > TOLERANCE || overflow_top > TOLERANCE {
                let overflow = overflow_right.max(overflow_bottom).max(overflow_left).max(overflow_top);
                if is_btn {
                    if !button_overflow_reported {
                        button_overflow_reported = true;
                        out.push(json!({
                            "kind": "button-overflow",
                            "id": n.id.0,
                            "name": n.name,
                            "detail": format!(
                                "button '{}' ({} x {} px) is too small for its label '{}' ({} x {} px at ({:.0}, {:.0})), overflowing by {:.0} px",
                                n.name, parent_w.round(), parent_h.round(), c.name, c_w.round(), c_h.round(), c_x.round(), c_y.round(), overflow.round()
                            ),
                            "fix": format!(
                                "set sizingH: \"hug\" on button id {} (or widen it to at least {:.0} px) so the button contains its text with padding",
                                n.id.0, (c_x + c_w + c_x.max(0.0)).ceil()
                            ),
                        }));
                    }
                } else if is_card || has_autolayout {
                    out.push(json!({
                        "kind": "overflowing-child",
                        "id": c.id.0,
                        "name": c.name,
                        "detail": format!(
                            "layer '{}' ({} px wide at x={:.0}) overflows parent container '{}' (width {} px) by {:.0} px",
                            c.name, c_w.round(), c_x.round(), n.name, parent_w.round(), overflow.round()
                        ),
                        "fix": format!("widen container '{}' (id {}) or set its sizing to 'hug' / auto-resize", n.name, n.id.0),
                    }));
                }
            }
        }
    }

    // A card or tile in a collection or product row with no image fill or image child
    if is_card && !has_image_descendant(doc, n) && parent_w >= 100.0 && parent_h >= 100.0 {
        out.push(json!({
            "kind": "missing-image",
            "id": n.id.0,
            "name": n.name,
            "detail": format!("{} x {} card '{}' has no photograph or image fill", parent_w.round(), parent_h.round(), n.name),
            "fix": format!("call source_image with layer id {} and a specific subject to place real photography", n.id.0),
        }));
    }
}

/// Whether a layer's name marks it as a button or interactive CTA.
fn is_button(n: &figcraft_doc::Node) -> bool {
    const BUTTON_MARKS: [&str; 4] = ["button", "btn", "cta", "action"];
    n.name.split(|c: char| !c.is_alphanumeric()).any(|w| !w.is_empty() && BUTTON_MARKS.iter().any(|m| w.eq_ignore_ascii_case(m)))
}

/// Whether a layer's name marks it as a card, tile or product showcase.
fn is_card_or_tile(n: &figcraft_doc::Node) -> bool {
    const CARD_MARKS: [&str; 7] = ["card", "tile", "product", "item", "merchandise", "editorial", "showcase"];
    n.name.split(|c: char| !c.is_alphanumeric()).any(|w| !w.is_empty() && CARD_MARKS.iter().any(|m| w.eq_ignore_ascii_case(m)))
}

/// Whether a node or any of its descendants carries an image fill or picture placeholder.
fn has_image_descendant(doc: &figcraft_doc::Document, n: &figcraft_doc::Node) -> bool {
    if has_image_paint(n) || names_a_picture(n) {
        return true;
    }
    for child_id in &n.children {
        if let Some(c) = doc.node(*child_id)
            && has_image_descendant(doc, c)
        {
            return true;
        }
    }
    false
}

/// Whether a layer is filled with an image.
fn has_image_paint(n: &figcraft_doc::Node) -> bool {
    n.fills.iter().any(|f| matches!(f.kind, figcraft_color::PaintKind::Image(_)))
}

/// Whether a layer's name marks it as a place a picture goes. A model that draws a grey rectangle
/// and names it `IMG·hero`, `Hero photo` or `Cover image` is asking for photography in the only
/// vocabulary it has; the audit reads that intent rather than letting the grey box through.
///
/// Whole words only. A substring match would read "discover" as a cover and "photography" as a
/// photo, so a headline about photography would be reported as a missing picture.
fn names_a_picture(n: &figcraft_doc::Node) -> bool {
    const MARKS: [&str; 10] = ["img", "image", "photo", "pic", "picture", "cover", "thumb", "media", "visual", "shot"];
    n.name.split(|c: char| !c.is_alphanumeric()).any(|word| !word.is_empty() && MARKS.iter().any(|m| word.eq_ignore_ascii_case(m)))
}

/// The text faults: a box too small for its own characters.
fn text_fault(doc: &figcraft_doc::Document, n: &figcraft_doc::Node) -> Option<Value> {
    let figcraft_doc::NodeKind::Text(t) = &n.kind else { return None };
    if t.characters.trim().is_empty() {
        return Some(json!({
            "kind": "empty-text", "id": n.id.0, "name": n.name,
            "detail": "a text layer with nothing in it",
            "fix": "write the copy, or delete the layer",
        }));
    }
    // What the text wants, unwrapped and wrapped at the box it has.
    let natural = figcraft_text::layout_cached(t, None, None);
    let natural_w = natural.width.ceil();
    let box_w = n.width;
    let wrapped = figcraft_text::layout_cached(t, Some(box_w.max(1.0)), None);
    let needs_h = wrapped.height;

    use figcraft_doc::TextAutoResize;
    match t.auto_resize {
        // The box grows with the text, so there is nothing to clip — unless the transform squashes
        // it. Leave that to the reviewer's eye.
        TextAutoResize::WidthAndHeight => None,
        // Fixed width, height grows: the width is the constraint. A word longer than the column
        // still spills, and that is the fault worth naming.
        TextAutoResize::Height => {
            let longest = wrapped.lines.iter().map(|l| l.width).fold(0.0_f64, f64::max);
            if longest > box_w + TOLERANCE && !wrapped.lines.is_empty() {
                Some(json!({
                    "kind": "text-wider-than-box", "id": n.id.0, "name": n.name,
                    "detail": format!("a line needs {:.0} px in a {} px box", longest, box_w.round()),
                    "fix": "widen the box, lower the font size, or allow the text to fill its container (sizingH: \"fill\")",
                }))
            } else {
                None
            }
        }
        // Fixed both ways: the height is the constraint, and text past it is invisible.
        TextAutoResize::None | TextAutoResize::Truncate => {
            let overflow_h = needs_h - n.height;
            let overflow_w = natural_w - box_w;
            if t.auto_resize == TextAutoResize::Truncate {
                return if overflow_w > TOLERANCE {
                    Some(json!({
                        "kind": "text-truncated", "id": n.id.0, "name": n.name,
                        "detail": format!("{natural_w:.0} px of one line cut to {} px, so it ends in an ellipsis", box_w.round()),
                        "fix": "widen the box, lower the font size, or shorten the copy",
                    }))
                } else {
                    None
                };
            }
            if overflow_h > TOLERANCE || overflow_w > TOLERANCE {
                let mut detail = format!("{:.0} x {:.0} px of text in a {} x {} box", natural_w, needs_h, box_w.round(), n.height.round());
                if overflow_h > TOLERANCE {
                    detail.push_str(&format!(", so {:.0} px of height is cut off", overflow_h));
                }
                Some(json!({
                    "kind": "text-clipped", "id": n.id.0, "name": n.name,
                    "detail": detail,
                    "fix": "set textAutoResize: \"height\" so the box grows with the copy, or make it fill its container",
                }))
            } else {
                let _ = doc;
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn session() -> Session {
        let mut s = Session::empty();
        let _ = s.execute("file.new", &json!({}));
        s
    }

    fn audit(s: &mut Session, params: Value) -> Value {
        let spec = specs().into_iter().find(|c| c.id == "layout.audit").expect("the audit is a command");
        (spec.run)(s, &params).expect("the audit runs")
    }

    /// A frame, and a text layer inside it fixed to a box the copy does not fit.
    #[test]
    fn text_fixed_to_a_box_it_does_not_fit_is_reported_with_its_numbers() {
        let mut s = session();
        let frame = s.execute("node.create", &json!({"type": "frame", "x": 0, "y": 0, "width": 566, "height": 400})).expect("a frame");
        let frame_id = frame["id"].clone();
        let text = s
            .execute(
                "text.create",
                &json!({"parent": frame_id, "x": 0, "y": 0, "characters": "One kiln. Four hands. No shortcuts.", "props": {"fontSize": 40}}),
            )
            .expect("text");
        let text_id = text["id"].as_i64().expect("an id");
        // Auto width says the box is as wide as the sentence: 666 px in a 566 px parent. Pinning
        // the width while keeping the height is the mistake a model makes.
        s.execute("node.set", &json!({"ids": [text_id], "props": {"textAutoResize": "none", "width": 566, "height": 48}})).expect("pin it");

        let report = audit(&mut s, json!({}));
        let found: Vec<&Value> = report["findings"].as_array().expect("findings").iter().filter(|f| f["id"] == text_id).collect();
        assert!(!found.is_empty(), "the clipped text is reported: {report}");
        let clipped = found.iter().any(|f| f["kind"] == "text-clipped");
        assert!(clipped, "it is reported as clipped: {report}");
        let detail = found.iter().find(|f| f["kind"] == "text-clipped").and_then(|f| f["detail"].as_str()).unwrap_or_default();
        assert!(detail.contains("566"), "the numbers are in the finding: {detail}");
        assert!(detail.contains("48"), "the height it does not fit is named too: {detail}");
    }

    #[test]
    fn a_text_box_that_grows_with_its_copy_is_not_a_fault() {
        let mut s = session();
        let frame = s.execute("node.create", &json!({"type": "frame", "x": 0, "y": 0, "width": 566, "height": 400})).expect("a frame");
        let frame_id = frame["id"].clone();
        // Height resize mode: the box takes the wrapped height, so nothing is cut.
        let text = s
            .execute(
                "text.create",
                &json!({"parent": frame_id, "x": 0, "y": 0, "characters": "One kiln. Four hands. No shortcuts.", "props": {"fontSize": 40, "textAutoResize": "height", "width": 566}}),
            )
            .expect("text");
        let text_id = text["id"].as_i64().expect("an id");
        let report = audit(&mut s, json!({}));
        let found: Vec<&Value> = report["findings"].as_array().expect("findings").iter().filter(|f| f["id"] == text_id).collect();
        assert!(found.is_empty(), "growing text is not a fault: {found:?}");
    }

    #[test]
    fn a_picture_box_with_no_picture_is_reported_and_the_fix_names_source_image() {
        let mut s = session();
        let frame = s.execute("node.create", &json!({"type": "frame", "x": 0, "y": 0, "width": 600, "height": 400})).expect("a frame");
        let frame_id = frame["id"].clone();
        let slot = s
            .execute(
                "node.create",
                &json!({"type": "rectangle", "parent": frame_id, "x": 0, "y": 0, "width": 300, "height": 200, "name": "Hero photo"}),
            )
            .expect("a slot");
        let slot_id = slot["id"].as_i64().expect("an id");
        let report = audit(&mut s, json!({}));
        let found = report["findings"].as_array().expect("findings").iter().find(|f| f["id"] == slot_id && f["kind"] == "empty-slot");
        let found = found.unwrap_or_else(|| panic!("the empty slot is reported: {report}"));
        assert!(found["fix"].as_str().unwrap_or_default().contains("source_image"), "the fix names the tool: {found}");
    }

    #[test]
    fn an_empty_text_layer_is_reported_rather_than_drawn() {
        let mut s = session();
        let t = s.execute("text.create", &json!({"x": 0, "y": 0, "characters": "", "props": {"fontSize": 16}})).expect("text");
        let id = t["id"].as_i64().expect("an id");
        let report = audit(&mut s, json!({}));
        assert!(report["findings"].as_array().expect("findings").iter().any(|f| f["id"] == id && f["kind"] == "empty-text"), "{report}");
    }

    #[test]
    fn a_headline_about_photography_is_not_a_missing_picture() {
        let mut s = session();
        let page =
            s.execute("node.create", &json!({"type": "frame", "x": 0, "y": 0, "width": 1440, "height": 900, "name": "Hero"})).expect("a frame");
        let page_id = page["id"].clone();
        // The failure this guards: matching a name on a substring read "Documentary photography in
        // Lisbon" as a picture slot, so the audit sent the model to `source_image` with a headline
        // for a subject. Two runs in a row spent their remaining steps chasing that.
        let heading = s
            .execute(
                "text.create",
                &json!({
                    "parent": page_id, "x": 80, "y": 120, "characters": "Documentary photography in Lisbon and the Alentejo. Film, mostly",
                    "props": {"fontSize": 64, "textAutoResize": "height", "width": 900},
                }),
            )
            .expect("a headline");
        let heading_id = heading["id"].as_i64().expect("an id");
        let report = audit(&mut s, json!({}));
        assert!(
            !report["findings"].as_array().expect("findings").iter().any(|f| f["id"] == heading_id && f["kind"] == "empty-slot"),
            "a headline is not a picture slot: {report}"
        );
    }

    #[test]
    fn a_picture_slot_is_one_finding_not_a_slot_and_a_container() {
        // A frame named as a picture and holding nothing is one fault: its photograph is missing.
        // Reporting it as an empty container too told the model to fill a container it had
        // already filled with the slot, spending half its fixing budget on a defect that was
        // not separate.
        let mut s = session();
        let slot = s
            .execute("node.create", &json!({"type": "frame", "x": 0, "y": 0, "width": 560, "height": 700, "name": "Hero photo slot"}))
            .expect("a slot");
        let slot_id = slot["id"].as_i64().expect("an id");
        let report = audit(&mut s, json!({}));
        let found: Vec<&Value> = report["findings"].as_array().expect("findings").iter().filter(|f| f["id"] == slot_id).collect();
        assert_eq!(found.len(), 1, "one grey box is one defect: {found:#?}");
        assert_eq!(found[0]["kind"], "empty-slot");
    }

    #[test]
    fn an_empty_section_is_still_a_container() {
        let mut s = session();
        let section =
            s.execute("node.create", &json!({"type": "frame", "x": 0, "y": 0, "width": 800, "height": 300, "name": "Prices"})).expect("a section");
        let id = section["id"].as_i64().expect("an id");
        let report = audit(&mut s, json!({}));
        let found: Vec<&Value> = report["findings"].as_array().expect("findings").iter().filter(|f| f["id"] == id).collect();
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0]["kind"], "empty-container");
    }

    #[test]
    fn a_frame_holding_a_picture_is_not_itself_a_slot() {
        let mut s = session();
        let page =
            s.execute("node.create", &json!({"type": "frame", "x": 0, "y": 0, "width": 1440, "height": 900, "name": "Cover"})).expect("a frame");
        let page_id = page["id"].clone();
        s.execute(
            "text.create",
            &json!({"parent": page_id, "x": 80, "y": 120, "characters": "Northgate", "props": {"fontSize": 64, "textAutoResize": "height", "width": 600}}),
        )
        .expect("a headline");
        let frame_id = page_id.as_i64().expect("an id");
        let report = audit(&mut s, json!({}));
        // The frame is named "Cover" and has a child: it is the section, and the slot inside it is
        // the thing that needs the picture.
        assert!(
            !report["findings"].as_array().expect("findings").iter().any(|f| f["id"] == frame_id && f["kind"] == "empty-slot"),
            "a container is not a slot: {report}"
        );
    }

    #[test]
    fn a_grey_box_named_img_hero_is_an_empty_slot() {
        let mut s = session();
        let page = s.execute("node.create", &json!({"type": "frame", "x": 0, "y": 0, "width": 720, "height": 720, "name": "Hero"})).expect("a frame");
        let slot = s
            .execute(
                "node.create",
                &json!({"type": "rectangle", "parent": page["id"].clone(), "x": 0, "y": 0, "width": 720, "height": 720, "name": "IMG·hero", "props": {"fills": ["#E7E2D8"]}}),
            )
            .expect("a slot");
        let slot_id = slot["id"].as_i64().expect("an id");
        let report = audit(&mut s, json!({}));
        let found = report["findings"].as_array().expect("findings").iter().find(|f| f["id"] == slot_id && f["kind"] == "empty-slot");
        let found = found.unwrap_or_else(|| panic!("a grey box named IMG·hero is an empty picture slot: {report}"));
        assert!(found["fix"].as_str().unwrap_or_default().contains("source_image"), "the fix names the tool: {found}");
    }

    #[test]
    fn a_section_frame_with_nothing_in_it_is_reported() {
        let mut s = session();
        let band = s
            .execute("node.create", &json!({"type": "frame", "x": 0, "y": 0, "width": 1440, "height": 192, "name": "Band · Rooms"}))
            .expect("a band");
        let band_id = band["id"].as_i64().expect("an id");
        let report = audit(&mut s, json!({}));
        assert!(
            report["findings"].as_array().expect("findings").iter().any(|f| f["id"] == band_id && f["kind"] == "empty-container"),
            "an empty section is reported: {report}"
        );
    }

    #[test]
    fn a_section_with_a_heading_in_it_is_not_empty() {
        let mut s = session();
        let band = s
            .execute("node.create", &json!({"type": "frame", "x": 0, "y": 0, "width": 1440, "height": 192, "name": "Band · Rooms"}))
            .expect("a band");
        let band_id = band["id"].as_i64().expect("an id");
        s.execute("text.create", &json!({"parent": band_id, "x": 64, "y": 40, "characters": "Explore the rooms", "props": {"fontSize": 24}}))
            .expect("a heading");
        let report = audit(&mut s, json!({}));
        assert!(
            !report["findings"].as_array().expect("findings").iter().any(|f| f["id"] == band_id && f["kind"] == "empty-container"),
            "a section with a heading in it is not empty: {report}"
        );
    }

    #[test]
    fn the_audit_changes_nothing() {
        let mut s = session();
        let before = s.execute("node.create", &json!({"type": "frame", "x": 0, "y": 0, "width": 100, "height": 100})).expect("a frame");
        let id = before["id"].clone();
        let undo_before = s.undo_depth().1;
        let _ = audit(&mut s, json!({}));
        assert_eq!(s.undo_depth().1, undo_before, "reading the document is not an undo step");
        let read = s.execute("node.get", &json!({"id": id.clone()})).expect("read back");
        assert_eq!(read["id"], id);
    }

    #[test]
    fn a_frame_asked_for_by_id_is_the_only_thing_audited() {
        let mut s = session();
        let a = s.execute("node.create", &json!({"type": "frame", "x": 0, "y": 0, "width": 100, "height": 100})).expect("a");
        let a_id = a["id"].as_i64().expect("an id");
        let b = s.execute("node.create", &json!({"type": "frame", "x": 200, "y": 0, "width": 100, "height": 100})).expect("b");
        let b_id = b["id"].as_i64().expect("an id");
        s.execute("text.create", &json!({"parent": b_id, "x": 0, "y": 0, "characters": "", "props": {"fontSize": 16}})).expect("an empty text in b");
        let report = audit(&mut s, json!({"ids": [a_id]}));
        let ids: Vec<i64> = report["findings"].as_array().expect("findings").iter().filter_map(|f| f["id"].as_i64()).collect();
        assert!(!ids.contains(&b_id), "b was not asked for: {report}");
    }

    /// A shape the design left loose on the page, outside every frame, is a real fault: it lands
    /// at the top-left of the exported image and appears in nobody's layout.
    #[test]
    fn a_shape_loose_on_the_page_is_reported_even_though_it_makes_no_text_clip() {
        let mut s = session();
        s.execute("node.create", &json!({"type": "frame", "x": 0, "y": 0, "width": 1440, "height": 900})).expect("a frame");
        let loose = s.execute("node.create", &json!({"type": "rectangle", "x": 0, "y": 0, "width": 371, "height": 464})).expect("a rectangle");
        let loose_id = loose["id"].as_i64().expect("an id");
        s.execute("node.set", &json!({"ids": [loose_id], "props": {"name": "Terra bowl photo"}})).expect("name it");

        let report = audit(&mut s, json!({}));
        let found: Vec<&Value> = report["findings"].as_array().expect("findings").iter().filter(|f| f["id"] == loose_id).collect();
        assert!(found.iter().any(|f| f["kind"] == "stray-layer"), "the loose shape is reported: {report}");
        assert_eq!(report["counts"]["strayLayer"], 1, "{report}");
    }

    #[test]
    fn the_params_are_documented() {
        let spec = specs().into_iter().find(|c| c.id == "layout.audit").expect("the audit is a command");
        assert!(spec.params.contains("ids"), "the doc names `ids`");
        assert!(spec.params.contains("depth"), "the doc names `depth`");
        assert!(!spec.history, "reading a document is not undoable");
    }

    #[test]
    fn a_button_whose_label_overflows_is_reported() {
        let mut s = session();
        let btn =
            s.execute("node.create", &json!({"type": "frame", "x": 0, "y": 0, "width": 100, "height": 46, "name": "Button primary"})).expect("btn");
        let btn_id = btn["id"].as_i64().expect("an id");
        s.execute(
            "text.create",
            &json!({"parent": btn_id, "x": 30, "y": 15, "characters": "Shop the collection", "props": {"fontSize": 16, "width": 164}}),
        )
        .expect("text");

        let report = audit(&mut s, json!({}));
        let found = report["findings"].as_array().expect("findings").iter().find(|f| f["id"] == btn_id && f["kind"] == "button-overflow");
        let found = found.unwrap_or_else(|| panic!("button overflow is reported: {report}"));
        assert!(found["detail"].as_str().unwrap_or_default().contains("Button primary"), "{found}");
        assert!(found["fix"].as_str().unwrap_or_default().contains("sizingH"), "{found}");
    }

    #[test]
    fn a_card_tile_with_no_image_is_reported_as_missing_image() {
        let mut s = session();
        let tile = s
            .execute(
                "node.create",
                &json!({"type": "frame", "x": 0, "y": 0, "width": 393, "height": 460, "name": "Tile — Work", "props": {"fills": ["#E6DED1"]}}),
            )
            .expect("tile");
        let tile_id = tile["id"].as_i64().expect("an id");

        let report = audit(&mut s, json!({}));
        let found = report["findings"].as_array().expect("findings").iter().find(|f| f["id"] == tile_id && f["kind"] == "missing-image");
        let found = found.unwrap_or_else(|| panic!("missing image is reported: {report}"));
        assert!(found["fix"].as_str().unwrap_or_default().contains("source_image"), "{found}");
    }

    #[test]
    fn an_autolayout_container_whose_child_overflows_is_reported() {
        let mut s = session();
        let container = s.execute("node.create", &json!({"type": "frame", "x": 0, "y": 0, "width": 200, "height": 100, "name": "Card container", "props": {"layoutMode": "horizontal"}})).expect("frame");
        let c_id = container["id"].as_i64().expect("an id");
        let child = s
            .execute("node.create", &json!({"type": "rectangle", "parent": c_id, "x": 10, "y": 0, "width": 250, "height": 80, "name": "Wide inner"}))
            .expect("child");
        let child_id = child["id"].as_i64().expect("an id");

        let report = audit(&mut s, json!({}));
        let found = report["findings"].as_array().expect("findings").iter().find(|f| f["id"] == child_id && f["kind"] == "overflowing-child");
        assert!(found.is_some(), "child overflow in autolayout container is reported: {report}");
    }
}
