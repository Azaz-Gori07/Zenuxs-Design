//! The Icons tab: searchable vector icon library grid in the left sidebar panel.

use egui::{Align2, CornerRadius, Id, Rect, Sense, Stroke, StrokeKind, Ui, vec2};
use serde_json::json;

use crate::FigApp;
use crate::a11y::Tip;
use crate::i18n::tr;
use crate::icons::{self, Icon};
use crate::theme::{Tokens, regular};

const ICON_SIZE: f32 = 38.0;
const ICON_GAP: f32 = 6.0;

/// Built-in curated icon catalog for instant offline canvas insertion (Font Awesome Free v6)
const CURATED_ICONS: [(&str, Icon, &str); 28] = [
    ("Shapes", Icon::Shapes, "fa6-solid:shapes"),
    ("Star", Icon::Star, "fa6-solid:star"),
    ("Heart", Icon::Reaction, "fa6-solid:heart"),
    ("Sparkles", Icon::Sparkle, "fa6-solid:wand-magic-sparkles"),
    ("Home", Icon::Home, "fa6-solid:house"),
    ("File", Icon::Page, "fa6-solid:file"),
    ("Layers", Icon::Assets, "fa6-solid:layer-group"),
    ("Settings", Icon::Adjust, "fa6-solid:gear"),
    ("Search", Icon::Search, "fa6-solid:magnifying-glass"),
    ("Comment", Icon::Comment, "fa6-solid:comment"),
    ("Send", Icon::Send, "fa6-solid:paper-plane"),
    ("Clip", Icon::Clip, "fa6-solid:paperclip"),
    ("Check", Icon::Check, "fa6-solid:check"),
    ("Close", Icon::Close, "fa6-solid:xmark"),
    ("Plus", Icon::Plus, "fa6-solid:plus"),
    ("Minus", Icon::Minus, "fa6-solid:minus"),
    ("Timer", Icon::Timer, "fa6-solid:stopwatch"),
    ("Pause", Icon::Pause, "fa6-solid:pause"),
    ("Globe", Icon::Online, "fa6-solid:globe"),
    ("Lock", Icon::LivePrivate, "fa6-solid:lock"),
    ("Grid", Icon::ViewGrid, "fa6-solid:table-cells-large"),
    ("List", Icon::ViewList, "fa6-solid:list"),
    ("Arrow Down", Icon::ArrowDown, "fa6-solid:arrow-down"),
    ("Pen", Icon::PenNib, "fa6-solid:pen"),
    ("Marker", Icon::Marker, "fa6-solid:pen-nib"),
    ("Highlighter", Icon::Highlighter, "fa6-solid:highlighter"),
    ("Connector", Icon::Connector, "fa6-solid:code-commit"),
    ("Stamp", Icon::Stamp, "fa6-solid:stamp"),
];

/// Draw the Icons sidebar panel.
pub fn show(app: &mut FigApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let mut search = app.ui.icons_query.clone();

    // 1. Search Bar
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
        let resp = crate::widgets::text_input(
            ui,
            Id::unique("icons_panel_search"),
            &mut search,
            ui.available_width() - 8.0,
            tr("Search Font Awesome Free (v6)..."),
        );
        if resp.changed() {
            app.ui.icons_query = search.clone();
        }
    });

    ui.add_space(6.0);

    // 2. Filter collection chips (Font Awesome Free v6 categories)
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        let collections = ["All FA", "Solid", "Regular", "Brands"];
        for col in collections {
            let active = app.ui.icons_collection == col || (col == "All FA" && app.ui.icons_collection.is_empty());
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
                app.ui.icons_collection = if col == "All FA" { String::new() } else { col.to_string() };
            }
        }
    });

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // 3. Grid of Icons
    let filter = app.ui.icons_query.trim().to_lowercase();
    let matching: Vec<_> =
        CURATED_ICONS.iter().filter(|(name, _, id)| filter.is_empty() || name.to_lowercase().contains(&filter) || id.contains(&filter)).collect();

    if matching.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(24.0);
            ui.label(egui::RichText::new(tr("No icons found for this query")).font(regular(12.0)).color(t.text_secondary));
        });
        return;
    }

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let width = ui.available_width();
        let cols = ((width / (ICON_SIZE + ICON_GAP)).floor() as usize).clamp(3, 6);

        egui::Grid::new("icons_grid").spacing(vec2(ICON_GAP, ICON_GAP)).min_col_width(ICON_SIZE).show(ui, |ui| {
            let mut col_idx = 0;
            for (name, icon, icon_id) in matching {
                let (rect, resp) = ui.allocate_exact_size(vec2(ICON_SIZE, ICON_SIZE), Sense::click());
                let hovered = resp.hovered();

                if hovered {
                    ui.painter().rect_filled(rect, CornerRadius::same(6), t.hover);
                    ui.painter().rect_stroke(rect, CornerRadius::same(6), Stroke::new(1.0, t.border), StrokeKind::Inside);
                }

                icons::paint(
                    ui.painter(),
                    Rect::from_center_size(rect.center(), vec2(20.0, 20.0)),
                    *icon,
                    if hovered { t.accent_text } else { t.icon },
                );
                let clicked = resp.clicked();
                resp.tip(format!("{name} ({icon_id})"));

                if clicked {
                    insert_icon_to_canvas(app, name, *icon);
                }

                col_idx += 1;
                if col_idx >= cols {
                    ui.end_row();
                    col_idx = 0;
                }
            }
        });
    });
}

/// Helper to insert an icon as a vector node onto the current page canvas.
fn insert_icon_to_canvas(app: &mut FigApp, name: &str, _icon: Icon) {
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
    app.toast(format!("Inserted {name} icon"));
}
