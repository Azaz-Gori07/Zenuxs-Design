//! The Icons tab: searchable Font Awesome Free (v6) vector icon library in the left sidebar panel.

use egui::{Align2, CornerRadius, Id, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::json;

use super::fa6_icons::FA6_ICONS;
use crate::FigApp;
use crate::a11y::Tip;
use crate::i18n::tr;
use crate::icons::{self, Icon};
use crate::theme::{Tokens, regular};

const ICON_SIZE: f32 = 38.0;
const ICON_GAP: f32 = 6.0;

/// Map Font Awesome icon names and semantic categories to visual vector icons
fn map_fa_icon(name: &str) -> Icon {
    let n = name.to_lowercase();
    if n.contains("house") || n.contains("home") || n.contains("building") {
        return Icon::Home;
    }
    if n.contains("star") || n.contains("award") || n.contains("badge") || n.contains("certificate") {
        return Icon::Star;
    }
    if n.contains("heart") || n.contains("thumb") || n.contains("face") || n.contains("smile") || n.contains("music") {
        return Icon::Reaction;
    }
    if n.contains("search") || n.contains("glass") || n.contains("find") {
        return Icon::Search;
    }
    if n.contains("gear") || n.contains("cog") || n.contains("setting") || n.contains("slider") || n.contains("wrench") || n.contains("filter") {
        return Icon::Adjust;
    }
    if n.contains("bell") || n.contains("alarm") {
        return Icon::Bell;
    }
    if n.contains("check") {
        return Icon::Check;
    }
    if n.contains("xmark") || n.contains("close") || n.contains("cross") || n.contains("ban") || n.contains("trash") || n.contains("delete") {
        return Icon::Close;
    }
    if n.contains("plus") || n.contains("add") {
        return Icon::Plus;
    }
    if n.contains("minus") || n.contains("remove") || n.contains("dash") {
        return Icon::Minus;
    }
    if n.contains("comment") || n.contains("message") || n.contains("chat") || n.contains("quote") {
        return Icon::Comment;
    }
    if n.contains("send") || n.contains("plane") || n.contains("share") {
        return Icon::Send;
    }
    if n.contains("clip") || n.contains("attach") {
        return Icon::Clip;
    }
    if n.contains("unlock") {
        return Icon::Unlock;
    }
    if n.contains("lock") || n.contains("shield") || n.contains("key") {
        return Icon::Lock;
    }
    if n.contains("eye-slash") {
        return Icon::EyeOff;
    }
    if n.contains("eye") {
        return Icon::Eye;
    }
    if n.contains("globe")
        || n.contains("earth")
        || n.contains("wifi")
        || n.contains("network")
        || n.contains("brand")
        || n.contains("twitter")
        || n.contains("github")
        || n.contains("google")
        || n.contains("apple")
        || n.contains("meta")
        || n.contains("facebook")
    {
        return Icon::Online;
    }
    if n.contains("clock") || n.contains("timer") || n.contains("stopwatch") || n.contains("calendar") || n.contains("hourglass") {
        return Icon::Timer;
    }
    if n.contains("pause") {
        return Icon::Pause;
    }
    if n.contains("play") || n.contains("forward") {
        return Icon::Play;
    }
    if n.contains("file") || n.contains("doc") || n.contains("page") || n.contains("note") {
        return Icon::Page;
    }
    if n.contains("folder") || n.contains("layer") || n.contains("box") || n.contains("archive") {
        return Icon::Assets;
    }
    if n.contains("grid") || n.contains("cell") || n.contains("table") || n.contains("layout") {
        return Icon::ViewGrid;
    }
    if n.contains("list") || n.contains("menu") || n.contains("bars") {
        return Icon::ViewList;
    }
    if n.contains("down") {
        return Icon::ArrowDown;
    }
    if n.contains("arrow") || n.contains("chevron") || n.contains("caret") || n.contains("angle") {
        return Icon::Arrow;
    }
    if n.contains("pen") || n.contains("draw") || n.contains("write") {
        return Icon::Pen;
    }
    if n.contains("pencil") || n.contains("brush") {
        return Icon::Pencil;
    }
    if n.contains("image") || n.contains("camera") || n.contains("photo") || n.contains("film") || n.contains("video") {
        return Icon::Image;
    }
    if n.contains("shape") || n.contains("cube") || n.contains("object") || n.contains("square") || n.contains("clone") {
        return Icon::Shapes;
    }
    if n.contains("question") || n.contains("help") || n.contains("info") {
        return Icon::Help;
    }
    if n.contains("user") || n.contains("person") || n.contains("people") || n.contains("profile") || n.contains("avatar") || n.contains("id") {
        return Icon::Instance;
    }
    if n.contains("code") || n.contains("terminal") || n.contains("laptop") || n.contains("desktop") {
        return Icon::Code;
    }
    if n.contains("book") || n.contains("tag") || n.contains("bookmark") {
        return Icon::Library;
    }
    if n.contains("sun") || n.contains("light") || n.contains("bolt") || n.contains("fire") {
        return Icon::Sun;
    }
    if n.contains("moon") || n.contains("dark") || n.contains("cloud") {
        return Icon::Moon;
    }
    if n.contains("circle") {
        return Icon::Ellipse;
    }
    Icon::Vector
}

/// Draw the Icons sidebar panel with Font Awesome Free (v6).
pub fn show(app: &mut FigApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());

    // 1. Search Box with magnifier icon and proper field frame
    let (fr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::hover());
    let field = Rect::from_min_max(pos2(fr.min.x + 8.0, fr.min.y + 4.0), pos2(fr.max.x - 8.0, fr.max.y - 4.0));
    let r = crate::widgets::search_box(
        ui,
        field,
        Id::unique("icons_panel_search_field"),
        &mut app.ui.icons_query,
        tr("Search Font Awesome Free (v6)..."),
    );
    if r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.ui.icons_query.clear();
    }

    ui.add_space(6.0);

    // 2. Filter collection chips (Font Awesome Free v6 categories)
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        let collections = ["All FA", "Solid", "Regular", "Brands"];
        for col in collections {
            let is_all = col == "All FA";
            let active = app.ui.icons_collection == col || (is_all && app.ui.icons_collection.is_empty());
            let fill = if active { t.accent_on } else { t.hover };
            let text_color = if active { t.accent_text } else { t.text };
            let chip_r =
                ui.allocate_response(vec2(ui.painter().layout_no_wrap(col.into(), regular(11.0), text_color).size().x + 12.0, 20.0), Sense::click());
            if chip_r.hovered() || active {
                ui.painter().rect_filled(chip_r.rect, CornerRadius::same(10), fill);
            } else {
                ui.painter().rect_stroke(chip_r.rect, CornerRadius::same(10), Stroke::new(1.0, t.border), StrokeKind::Inside);
            }
            ui.painter().text(chip_r.rect.center(), Align2::CENTER_CENTER, col, regular(11.0), text_color);
            if chip_r.clicked() {
                app.ui.icons_collection = if is_all { String::new() } else { col.to_string() };
            }
        }
    });

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // 3. Grid of Icons: filtered by query AND collection category
    let filter = app.ui.icons_query.trim().to_lowercase();
    let selected_category = app.ui.icons_collection.as_str();

    let matching: Vec<_> = FA6_ICONS
        .iter()
        .filter(|(name, id, cat)| {
            let matches_category = match selected_category {
                "Solid" => *cat == "Solid",
                "Regular" => *cat == "Regular",
                "Brands" => *cat == "Brands",
                _ => true,
            };
            let matches_query = filter.is_empty() || name.to_lowercase().contains(&filter) || id.contains(&filter);
            matches_category && matches_query
        })
        .collect();

    if matching.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(24.0);
            ui.label(egui::RichText::new(tr("No Font Awesome icons found for this query")).font(regular(12.0)).color(t.text_secondary));
        });
        return;
    }

    // Results count
    let count_text = format!("{} icons", matching.len());
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(egui::RichText::new(count_text).font(regular(10.0)).color(t.text_tertiary));
    });
    ui.add_space(4.0);

    let width = ui.available_width();
    let cols = ((width / (ICON_SIZE + ICON_GAP)).floor() as usize).clamp(3, 6);
    let row_height = ICON_SIZE + ICON_GAP;
    let total_rows = matching.len().div_ceil(cols);

    egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, row_height, total_rows, |ui, row_range| {
        for row in row_range {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(ICON_GAP, 0.0);
                let start = row * cols;
                let end = (start + cols).min(matching.len());
                for idx in start..end {
                    let (name, icon_id, _cat) = matching[idx];
                    let (rect, resp) = ui.allocate_exact_size(vec2(ICON_SIZE, ICON_SIZE), Sense::click());
                    let hovered = resp.hovered();

                    if hovered {
                        ui.painter().rect_filled(rect, CornerRadius::same(6), t.hover);
                        ui.painter().rect_stroke(rect, CornerRadius::same(6), Stroke::new(1.0, t.border), StrokeKind::Inside);
                    }

                    let ic = map_fa_icon(name);
                    icons::paint(
                        ui.painter(),
                        Rect::from_center_size(rect.center(), vec2(20.0, 20.0)),
                        ic,
                        if hovered { t.accent_text } else { t.icon },
                    );

                    let clicked = resp.clicked();
                    resp.tip(format!("{name} ({icon_id})"));

                    if clicked {
                        insert_icon_to_canvas(app, name, icon_id);
                    }
                }
            });
            ui.add_space(ICON_GAP);
        }
    });
}

/// Helper to insert an icon as a vector node onto the current page canvas.
fn insert_icon_to_canvas(app: &mut FigApp, name: &str, icon_id: &str) {
    let Ok(ds) = app.session.ds() else { return };
    let parent = ds.page;
    let (center_x, center_y) = (ds.view.cx, ds.view.cy);

    let _ = app.session.execute(
        "node.create",
        &json!({
            "type": "frame",
            "name": format!("Icon / {name}"),
            "x": center_x - 16.0,
            "y": center_y - 16.0,
            "width": 32.0,
            "height": 32.0,
            "parent": parent.0,
            "props": {
                "fills": [{"type": "solid", "color": "#bfbcfc15"}],
                "cornerRadius": 6.0,
            }
        }),
    );
    app.toast(format!("Inserted {name} ({icon_id})"));
}
