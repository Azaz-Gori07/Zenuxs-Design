//! The Market tab: in-app prebuilt component marketplace for the Shadcn ecosystem.

use egui::{Align2, CornerRadius, Id, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::json;

use crate::FigApp;
use crate::theme::{Tokens, regular, semibold};

struct MarketItemDef {
    id: &'static str,
    name: &'static str,
    source: &'static str,
    cat: &'static str,
    desc: &'static str,
}

const PREBUILT_ITEMS: &[MarketItemDef] = &[
    MarketItemDef {
        id: "@shadcn/button",
        name: "Primary Button",
        source: "@shadcn",
        cat: "Buttons",
        desc: "Accessible interactive button with focus ring and rounded corners.",
    },
    MarketItemDef {
        id: "@originui/button-shimmer",
        name: "Shimmer Border Button",
        source: "@originui",
        cat: "Buttons",
        desc: "Polished button with animated gradient border and subtle glow.",
    },
    MarketItemDef {
        id: "@neobrutalism/neo-button",
        name: "Neo-Brutalist Button",
        source: "@neobrutalism",
        cat: "Buttons",
        desc: "High-contrast retro button with 2.5px border and hard shadow.",
    },
    MarketItemDef {
        id: "@cult/glow-button",
        name: "Cyberpunk Glow Button",
        source: "@cult",
        cat: "Buttons",
        desc: "Neon glowing button with radial pulsing gradient backdrop.",
    },
    MarketItemDef {
        id: "@shadcn/input",
        name: "Standard Text Input",
        source: "@shadcn",
        cat: "Inputs",
        desc: "Clean accessible text input with subtle border and focus ring.",
    },
    MarketItemDef {
        id: "@originui/input-search-kbd",
        name: "Search Input with ⌘K",
        source: "@originui",
        cat: "Inputs",
        desc: "Search bar with magnifying glass icon and ⌘K badge indicator.",
    },
    MarketItemDef {
        id: "@kibo/tags-input",
        name: "Multi-Tag Chips Input",
        source: "@kibo",
        cat: "Inputs",
        desc: "Interactive chips container to select and dismiss tags in forms.",
    },
    MarketItemDef {
        id: "@shadcn/card-metric",
        name: "KPI Stat Metric Card",
        source: "@shadcn",
        cat: "Cards",
        desc: "Executive dashboard stat card with value and percentage trend.",
    },
    MarketItemDef {
        id: "@originui/pricing-card",
        name: "Tiered Pricing Card",
        source: "@originui",
        cat: "Cards",
        desc: "Highlighted subscription pricing card with checklist and CTA.",
    },
    MarketItemDef {
        id: "@cult/bento-card",
        name: "Bento Showcase Card",
        source: "@cult",
        cat: "Cards",
        desc: "Modern Apple/Bento visual showcase card with icon header.",
    },
    MarketItemDef {
        id: "@shadcn/area-chart",
        name: "Revenue Area Chart",
        source: "@shadcn",
        cat: "Charts",
        desc: "Growth curve area chart card with gradient fill and date axes.",
    },
    MarketItemDef {
        id: "@originui/donut-chart",
        name: "Donut Category Chart",
        source: "@originui",
        cat: "Charts",
        desc: "Circular donut chart visualizing market share and legends.",
    },
    MarketItemDef {
        id: "@kibo/kanban-board",
        name: "Kanban Task Board",
        source: "@kibo",
        cat: "Kanban/Complex",
        desc: "3-column agile tracking board (To Do, In Progress, Done).",
    },
    MarketItemDef {
        id: "@bundui/data-table",
        name: "Enterprise Data Table",
        source: "@bundui",
        cat: "Kanban/Complex",
        desc: "Dense data table layout with sorting headers and status pills.",
    },
    MarketItemDef {
        id: "@animate-ui/marquee-ticker",
        name: "Marquee Ribbon Ticker",
        source: "@animate-ui",
        cat: "Animations",
        desc: "Infinite sliding marquee ribbon for partner logos and badges.",
    },
    MarketItemDef {
        id: "@animate-ui/spotlight-card",
        name: "Cursor Spotlight Card",
        source: "@animate-ui",
        cat: "Animations",
        desc: "Card with dynamic radial spotlight that follows pointer movement.",
    },
];

/// Draw the Marketplace explorer panel in the left sidebar.
pub fn show(app: &mut FigApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());

    // 1. Search Box
    let (fr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::hover());
    let field = Rect::from_min_max(pos2(fr.min.x + 8.0, fr.min.y + 4.0), pos2(fr.max.x - 8.0, fr.max.y - 4.0));
    let r =
        crate::widgets::search_box(ui, field, Id::unique("market_panel_search_field"), &mut app.ui.market_query, "Search prebuilt components...");
    if r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.ui.market_query.clear();
    }

    ui.add_space(4.0);

    // 2. Category filter pills
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        let categories = ["All", "Buttons", "Inputs", "Cards", "Charts", "Kanban/Complex", "Animations"];
        for cat in categories {
            let is_all = cat == "All";
            let active = app.ui.market_category == cat || (is_all && app.ui.market_category.is_empty());
            let fill = if active { t.accent_on } else { t.hover };
            let text_color = if active { t.accent_text } else { t.text };
            let chip_r =
                ui.allocate_response(vec2(ui.painter().layout_no_wrap(cat.into(), regular(11.0), text_color).size().x + 12.0, 20.0), Sense::click());
            if chip_r.hovered() || active {
                ui.painter().rect_filled(chip_r.rect, CornerRadius::same(10), fill);
            } else {
                ui.painter().rect_stroke(chip_r.rect, CornerRadius::same(10), Stroke::new(1.0, t.border), StrokeKind::Inside);
            }
            ui.painter().text(chip_r.rect.center(), Align2::CENTER_CENTER, cat, regular(11.0), text_color);
            if chip_r.clicked() {
                app.ui.market_category = if is_all { String::new() } else { cat.to_string() };
            }
        }
    });

    ui.add_space(4.0);

    // 3. Source Registry filter pills
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
        let registries = ["all", "@shadcn", "@originui", "@kibo", "@cult", "@animate-ui", "@bundui", "@neobrutalism"];
        for reg in registries {
            let is_all = reg == "all";
            let label = if is_all { "All Sources" } else { reg };
            let active = app.ui.market_registry == reg || (is_all && app.ui.market_registry.is_empty());
            let fill = if active { t.accent_on } else { t.hover };
            let text_color = if active { t.accent_text } else { t.text_secondary };
            let chip_r =
                ui.allocate_response(vec2(ui.painter().layout_no_wrap(label.into(), regular(10.0), text_color).size().x + 10.0, 18.0), Sense::click());
            if chip_r.hovered() || active {
                ui.painter().rect_filled(chip_r.rect, CornerRadius::same(6), fill);
            } else {
                ui.painter().rect_stroke(chip_r.rect, CornerRadius::same(6), Stroke::new(1.0, t.border), StrokeKind::Inside);
            }
            ui.painter().text(chip_r.rect.center(), Align2::CENTER_CENTER, label, regular(10.0), text_color);
            if chip_r.clicked() {
                app.ui.market_registry = if is_all { String::new() } else { reg.to_string() };
            }
        }
    });

    ui.add_space(6.0);
    ui.separator();
    ui.add_space(4.0);

    // 4. Component Card List
    let filter = app.ui.market_query.trim().to_lowercase();
    let selected_category = app.ui.market_category.as_str();
    let selected_registry = app.ui.market_registry.as_str();

    let matching: Vec<_> = PREBUILT_ITEMS
        .iter()
        .filter(|item| {
            let matches_cat = selected_category.is_empty() || item.cat == selected_category;
            let matches_reg = selected_registry.is_empty() || item.source == selected_registry;
            let matches_query = filter.is_empty()
                || item.name.to_lowercase().contains(&filter)
                || item.id.contains(&filter)
                || item.desc.to_lowercase().contains(&filter);
            matches_cat && matches_reg && matches_query
        })
        .collect();

    if matching.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(24.0);
            ui.label(egui::RichText::new("No components found matching search").font(regular(12.0)).color(t.text_secondary));
        });
        return;
    }

    let mut to_insert: Option<&'static str> = None;

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 8.0);
        for item in matching {
            let card_w = ui.available_width() - 8.0;
            let (card_rect, _) = ui.allocate_exact_size(vec2(card_w, 88.0), Sense::hover());
            ui.painter().rect_filled(card_rect, CornerRadius::same(8), t.panel);
            ui.painter().rect_stroke(card_rect, CornerRadius::same(8), Stroke::new(1.0, t.border), StrokeKind::Inside);

            // Title
            let title_pos = pos2(card_rect.min.x + 10.0, card_rect.min.y + 10.0);
            ui.painter().text(title_pos, Align2::LEFT_TOP, item.name, semibold(12.0), t.text);

            // Badge
            let badge_pos = pos2(card_rect.min.x + 10.0, card_rect.min.y + 28.0);
            ui.painter().text(badge_pos, Align2::LEFT_TOP, format!("{} • 100% Free", item.source), regular(10.0), t.accent);

            // Description
            let desc_pos = pos2(card_rect.min.x + 10.0, card_rect.min.y + 44.0);
            ui.painter().text(desc_pos, Align2::LEFT_TOP, item.desc, regular(10.0), t.text_secondary);

            // Add to Canvas Button
            let btn_rect = Rect::from_min_max(pos2(card_rect.max.x - 100.0, card_rect.max.y - 28.0), pos2(card_rect.max.x - 8.0, card_rect.max.y - 6.0));
            let btn_resp = ui.interact(btn_rect, ui.scope_id().with(("add_market", item.id)), Sense::click());
            let btn_fill = if btn_resp.hovered() { t.accent } else { t.accent_on };
            ui.painter().rect_filled(btn_rect, CornerRadius::same(4), btn_fill);
            ui.painter().text(btn_rect.center(), Align2::CENTER_CENTER, "+ Add to Canvas", semibold(10.0), t.accent_text);

            if btn_resp.clicked() {
                to_insert = Some(item.id);
            }
        }
    });

    if let Some(comp_id) = to_insert
        && let Ok(res) = figcraft_engine::chat_tools::instantiate_market_component(
            &mut app.session,
            comp_id,
            200.0,
            150.0,
            None,
            None,
        )
        && let Some(id) = res.get("id")
    {
        app.run("select.set", json!({ "ids": [id] }));
    }
}
