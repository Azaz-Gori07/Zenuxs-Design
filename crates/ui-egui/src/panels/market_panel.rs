//! The Market tab: in-app prebuilt component marketplace for the Shadcn ecosystem.

use egui::{Align2, Color32, CornerRadius, Id, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::json;

use crate::FigApp;
use crate::theme::{Tokens, regular, semibold};
use crate::widgets;

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
    let r = crate::widgets::search_box(ui, field, Id::unique("market_panel_search_field"), &mut app.ui.market_query, "Search prebuilt components...");
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
            let chip_r = ui
                .allocate_response(vec2(ui.painter().layout_no_wrap(label.into(), regular(10.0), text_color).size().x + 10.0, 18.0), Sense::click());
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
        ui.spacing_mut().item_spacing = vec2(0.0, 10.0);
        for item in matching {
            let card_w = ui.available_width() - 8.0;
            let (card_rect, _) = ui.allocate_exact_size(vec2(card_w, 168.0), Sense::hover());
            ui.painter().rect_filled(card_rect, CornerRadius::same(8), t.panel);
            ui.painter().rect_stroke(card_rect, CornerRadius::same(8), Stroke::new(1.0, t.border), StrokeKind::Inside);

            // 1. Header Row: Title & Source Badge
            let title_pos = pos2(card_rect.min.x + 10.0, card_rect.min.y + 10.0);
            ui.painter().text(title_pos, Align2::LEFT_TOP, item.name, semibold(12.0), t.text);

            let badge_pos = pos2(card_rect.max.x - 10.0, card_rect.min.y + 10.0);
            ui.painter().text(badge_pos, Align2::RIGHT_TOP, format!("{} • 100% Free", item.source), regular(10.0), t.accent);

            // 2. Exact Component Rendering / Visual Preview Box
            let preview_rect =
                Rect::from_min_max(pos2(card_rect.min.x + 8.0, card_rect.min.y + 30.0), pos2(card_rect.max.x - 8.0, card_rect.min.y + 92.0));
            paint_component_preview(ui.painter(), preview_rect, item.id, t.dark, t.text);

            // 3. Description
            let desc_pos = pos2(card_rect.min.x + 10.0, card_rect.min.y + 98.0);
            let desc_elided = widgets::elide_font(ui, item.desc, card_w - 20.0, regular(10.0));
            ui.painter().text(desc_pos, Align2::LEFT_TOP, &desc_elided, regular(10.0), t.text_secondary);

            // 4. Action Buttons (Bottom Row)
            let add_btn_rect =
                Rect::from_min_max(pos2(card_rect.max.x - 106.0, card_rect.max.y - 30.0), pos2(card_rect.max.x - 8.0, card_rect.max.y - 8.0));
            let add_resp = ui.interact(add_btn_rect, ui.scope_id().with(("add_market", item.id)), Sense::click());
            let add_fill = if add_resp.hovered() { t.accent } else { t.accent_on };
            ui.painter().rect_filled(add_btn_rect, CornerRadius::same(5), add_fill);
            ui.painter().text(add_btn_rect.center(), Align2::CENTER_CENTER, "+ Add to Canvas", semibold(10.5), t.accent_text);

            let preview_btn_rect =
                Rect::from_min_max(pos2(add_btn_rect.min.x - 68.0, card_rect.max.y - 30.0), pos2(add_btn_rect.min.x - 6.0, card_rect.max.y - 8.0));
            let preview_resp = ui.interact(preview_btn_rect, ui.scope_id().with(("prev_market", item.id)), Sense::click());
            let prev_fill = if preview_resp.hovered() { t.hover } else { t.field };
            ui.painter().rect_filled(preview_btn_rect, CornerRadius::same(5), prev_fill);
            ui.painter().rect_stroke(preview_btn_rect, CornerRadius::same(5), Stroke::new(1.0, t.border), StrokeKind::Inside);
            ui.painter().text(preview_btn_rect.center(), Align2::CENTER_CENTER, "Preview", regular(10.0), t.text);

            if add_resp.clicked() || preview_resp.clicked() {
                to_insert = Some(item.id);
            }
        }
    });

    if let Some(comp_id) = to_insert
        && let Ok(res) = figcraft_engine::chat_tools::instantiate_market_component(&mut app.session, comp_id, 200.0, 150.0, None, None)
        && let Some(id) = res.get("id")
    {
        app.run("select.set", json!({ "ids": [id] }));
    }
}

/// Paint an authentic visual miniature preview of a market component.
fn paint_component_preview(painter: &egui::Painter, rect: Rect, id: &str, dark: bool, default_text: Color32) {
    let bg = if dark { Color32::from_rgb(18, 19, 23) } else { Color32::from_rgb(245, 246, 248) };
    let border_col = if dark { Color32::from_rgb(38, 40, 48) } else { Color32::from_rgb(226, 232, 240) };
    painter.rect_filled(rect, CornerRadius::same(6), bg);
    painter.rect_stroke(rect, CornerRadius::same(6), Stroke::new(1.0, border_col), StrokeKind::Inside);

    let center = rect.center();

    match id {
        "@shadcn/button" => {
            let bw = 96.0;
            let bh = 26.0;
            let b_rect = Rect::from_center_size(center, vec2(bw, bh));
            let shadow_rect = b_rect.translate(vec2(0.0, 1.5));
            painter.rect_filled(shadow_rect, CornerRadius::same(6), Color32::from_rgba_unmultiplied(0, 0, 0, 35));
            painter.rect_filled(b_rect, CornerRadius::same(6), Color32::from_rgb(24, 24, 27));
            painter.rect_stroke(b_rect, CornerRadius::same(6), Stroke::new(1.0, Color32::from_rgb(63, 63, 70)), StrokeKind::Inside);
            painter.text(b_rect.center(), Align2::CENTER_CENTER, "Button →", semibold(11.0), Color32::WHITE);
        }
        "@originui/button-shimmer" => {
            let bw = 108.0;
            let bh = 26.0;
            let b_rect = Rect::from_center_size(center, vec2(bw, bh));
            let glow = b_rect.expand2(vec2(2.0, 1.5));
            painter.rect_filled(glow, CornerRadius::same(14), Color32::from_rgba_unmultiplied(59, 130, 246, 40));
            painter.rect_filled(b_rect, CornerRadius::same(13), Color32::from_rgb(9, 13, 22));
            painter.rect_stroke(b_rect, CornerRadius::same(13), Stroke::new(1.5, Color32::from_rgb(59, 130, 246)), StrokeKind::Inside);
            painter.text(b_rect.center(), Align2::CENTER_CENTER, "✦ Shimmer", semibold(11.0), Color32::from_rgb(147, 197, 253));
        }
        "@neobrutalism/neo-button" => {
            let bw = 96.0;
            let bh = 26.0;
            let front = Rect::from_center_size(center + vec2(-2.0, -2.0), vec2(bw, bh));
            let shadow = Rect::from_center_size(center + vec2(1.5, 1.5), vec2(bw, bh));
            painter.rect_filled(shadow, CornerRadius::same(4), Color32::BLACK);
            painter.rect_filled(front, CornerRadius::same(4), Color32::from_rgb(253, 224, 71));
            painter.rect_stroke(front, CornerRadius::same(4), Stroke::new(2.0, Color32::BLACK), StrokeKind::Inside);
            painter.text(front.center(), Align2::CENTER_CENTER, "⚡ NEO BTN", semibold(10.5), Color32::BLACK);
        }
        "@cult/glow-button" => {
            let bw = 106.0;
            let bh = 26.0;
            let b_rect = Rect::from_center_size(center, vec2(bw, bh));
            let halo = b_rect.expand2(vec2(3.0, 2.0));
            painter.rect_filled(halo, CornerRadius::same(8), Color32::from_rgba_unmultiplied(236, 72, 153, 50));
            painter.rect_filled(b_rect, CornerRadius::same(6), Color32::from_rgb(19, 4, 28));
            painter.rect_stroke(b_rect, CornerRadius::same(6), Stroke::new(1.5, Color32::from_rgb(236, 72, 153)), StrokeKind::Inside);
            painter.text(b_rect.center(), Align2::CENTER_CENTER, "✨ Cyber Glow", semibold(11.0), Color32::from_rgb(244, 114, 182));
        }
        "@shadcn/input" => {
            let iw = (rect.width() - 24.0).clamp(140.0, 210.0);
            let i_rect = Rect::from_center_size(center, vec2(iw, 26.0));
            let fill = if dark { Color32::from_rgb(26, 28, 34) } else { Color32::WHITE };
            painter.rect_filled(i_rect, CornerRadius::same(5), fill);
            painter.rect_stroke(i_rect, CornerRadius::same(5), Stroke::new(1.0, Color32::from_rgb(203, 213, 225)), StrokeKind::Inside);
            let text_pos = pos2(i_rect.min.x + 8.0, i_rect.center().y);
            painter.text(text_pos, Align2::LEFT_CENTER, "✉ email@example.com", regular(10.5), Color32::from_rgb(148, 163, 184));
        }
        "@originui/input-search-kbd" => {
            let iw = (rect.width() - 24.0).clamp(150.0, 210.0);
            let i_rect = Rect::from_center_size(center, vec2(iw, 26.0));
            let fill = if dark { Color32::from_rgb(26, 28, 34) } else { Color32::WHITE };
            painter.rect_filled(i_rect, CornerRadius::same(6), fill);
            painter.rect_stroke(i_rect, CornerRadius::same(6), Stroke::new(1.0, Color32::from_rgb(226, 232, 240)), StrokeKind::Inside);
            let text_pos = pos2(i_rect.min.x + 8.0, i_rect.center().y);
            painter.text(text_pos, Align2::LEFT_CENTER, "🔍 Search docs...", regular(10.5), Color32::from_rgb(148, 163, 184));
            let kbd_rect = Rect::from_min_size(pos2(i_rect.max.x - 28.0, i_rect.min.y + 4.0), vec2(22.0, 18.0));
            let kbd_fill = if dark { Color32::from_rgb(38, 42, 50) } else { Color32::from_rgb(241, 245, 249) };
            painter.rect_filled(kbd_rect, CornerRadius::same(4), kbd_fill);
            painter.rect_stroke(kbd_rect, CornerRadius::same(4), Stroke::new(1.0, Color32::from_rgb(203, 213, 225)), StrokeKind::Inside);
            painter.text(kbd_rect.center(), Align2::CENTER_CENTER, "⌘K", semibold(9.5), Color32::from_rgb(100, 116, 139));
        }
        "@kibo/tags-input" => {
            let iw = (rect.width() - 24.0).clamp(150.0, 210.0);
            let i_rect = Rect::from_center_size(center, vec2(iw, 28.0));
            let fill = if dark { Color32::from_rgb(26, 28, 34) } else { Color32::WHITE };
            painter.rect_filled(i_rect, CornerRadius::same(6), fill);
            painter.rect_stroke(i_rect, CornerRadius::same(6), Stroke::new(1.0, Color32::from_rgb(226, 232, 240)), StrokeKind::Inside);
            let t1 = Rect::from_min_size(pos2(i_rect.min.x + 6.0, i_rect.min.y + 4.5), vec2(46.0, 19.0));
            painter.rect_filled(t1, CornerRadius::same(4), Color32::from_rgb(243, 232, 255));
            painter.text(t1.center(), Align2::CENTER_CENTER, "React ✕", semibold(9.5), Color32::from_rgb(126, 34, 206));
            let t2 = Rect::from_min_size(pos2(t1.max.x + 5.0, i_rect.min.y + 4.5), vec2(54.0, 19.0));
            painter.rect_filled(t2, CornerRadius::same(4), Color32::from_rgb(224, 231, 255));
            painter.text(t2.center(), Align2::CENTER_CENTER, "Tailwind ✕", semibold(9.5), Color32::from_rgb(67, 56, 202));
        }
        "@shadcn/card-metric" => {
            let cw = (rect.width() - 20.0).clamp(150.0, 210.0);
            let c_rect = Rect::from_center_size(center, vec2(cw, 44.0));
            let fill = if dark { Color32::from_rgb(28, 30, 36) } else { Color32::WHITE };
            painter.rect_filled(c_rect, CornerRadius::same(6), fill);
            painter.rect_stroke(c_rect, CornerRadius::same(6), Stroke::new(1.0, Color32::from_rgb(226, 232, 240)), StrokeKind::Inside);
            let label_pos = pos2(c_rect.min.x + 8.0, c_rect.min.y + 7.0);
            painter.text(label_pos, Align2::LEFT_TOP, "Total Revenue", regular(9.0), Color32::from_rgb(100, 116, 139));
            let val_pos = pos2(c_rect.min.x + 8.0, c_rect.min.y + 20.0);
            let val_color = if dark { Color32::WHITE } else { Color32::from_rgb(15, 23, 42) };
            painter.text(val_pos, Align2::LEFT_TOP, "$48,250", semibold(13.0), val_color);
            let pill_rect = Rect::from_min_size(pos2(c_rect.max.x - 52.0, c_rect.center().y - 8.0), vec2(46.0, 16.0));
            painter.rect_filled(pill_rect, CornerRadius::same(8), Color32::from_rgb(220, 252, 231));
            painter.text(pill_rect.center(), Align2::CENTER_CENTER, "+18.4% ↑", semibold(9.0), Color32::from_rgb(21, 128, 61));
        }
        "@originui/pricing-card" => {
            let cw = (rect.width() - 20.0).clamp(150.0, 210.0);
            let c_rect = Rect::from_center_size(center, vec2(cw, 44.0));
            let fill = if dark { Color32::from_rgb(28, 30, 36) } else { Color32::WHITE };
            painter.rect_filled(c_rect, CornerRadius::same(8), fill);
            painter.rect_stroke(c_rect, CornerRadius::same(8), Stroke::new(1.5, Color32::from_rgb(99, 102, 241)), StrokeKind::Inside);
            let text_pos = pos2(c_rect.min.x + 8.0, c_rect.min.y + 6.0);
            painter.text(
                text_pos,
                Align2::LEFT_TOP,
                "Pro Plan • $29/mo",
                semibold(10.5),
                if dark { Color32::WHITE } else { Color32::from_rgb(15, 23, 42) },
            );
            let f_pos = pos2(c_rect.min.x + 8.0, c_rect.min.y + 22.0);
            painter.text(f_pos, Align2::LEFT_TOP, "✓ Unlimited canvas layers", regular(9.0), Color32::from_rgb(71, 85, 105));
            let btn_rect = Rect::from_min_size(pos2(c_rect.max.x - 48.0, c_rect.center().y - 8.0), vec2(42.0, 17.0));
            painter.rect_filled(btn_rect, CornerRadius::same(4), Color32::from_rgb(79, 70, 229));
            painter.text(btn_rect.center(), Align2::CENTER_CENTER, "Join →", semibold(9.0), Color32::WHITE);
        }
        "@cult/bento-card" => {
            let cw = (rect.width() - 20.0).clamp(150.0, 210.0);
            let c_rect = Rect::from_center_size(center, vec2(cw, 44.0));
            painter.rect_filled(c_rect, CornerRadius::same(8), Color32::from_rgb(23, 23, 23));
            painter.rect_stroke(c_rect, CornerRadius::same(8), Stroke::new(1.0, Color32::from_rgb(46, 46, 46)), StrokeKind::Inside);
            let icon_box = Rect::from_min_size(pos2(c_rect.min.x + 8.0, c_rect.min.y + 9.0), vec2(24.0, 24.0));
            painter.rect_filled(icon_box, CornerRadius::same(6), Color32::from_rgb(49, 46, 129));
            painter.text(icon_box.center(), Align2::CENTER_CENTER, "✦", semibold(12.0), Color32::from_rgb(165, 180, 252));
            let h_pos = pos2(icon_box.max.x + 8.0, c_rect.min.y + 8.0);
            painter.text(h_pos, Align2::LEFT_TOP, "Smart Bento Card", semibold(11.0), Color32::WHITE);
            let s_pos = pos2(icon_box.max.x + 8.0, c_rect.min.y + 23.0);
            painter.text(s_pos, Align2::LEFT_TOP, "Apple showcase grid layout", regular(9.0), Color32::from_rgb(163, 163, 163));
        }
        "@shadcn/area-chart" => {
            let cw = (rect.width() - 20.0).clamp(150.0, 210.0);
            let c_rect = Rect::from_center_size(center, vec2(cw, 44.0));
            let fill = if dark { Color32::from_rgb(28, 30, 36) } else { Color32::WHITE };
            painter.rect_filled(c_rect, CornerRadius::same(6), fill);
            painter.rect_stroke(c_rect, CornerRadius::same(6), Stroke::new(1.0, Color32::from_rgb(226, 232, 240)), StrokeKind::Inside);
            let base_y = c_rect.max.y - 7.0;
            painter.line_segment(
                [pos2(c_rect.min.x + 8.0, base_y), pos2(c_rect.max.x - 8.0, base_y)],
                Stroke::new(1.0, Color32::from_rgb(226, 232, 240)),
            );
            let bar_heights = [12.0, 18.0, 26.0, 20.0, 30.0];
            let bar_colors = [
                Color32::from_rgb(129, 140, 248),
                Color32::from_rgb(99, 102, 241),
                Color32::from_rgb(79, 70, 229),
                Color32::from_rgb(67, 56, 202),
                Color32::from_rgb(55, 48, 163),
            ];
            let start_x = c_rect.min.x + 16.0;
            let spacing = (c_rect.width() - 32.0) / 5.0;
            for (idx, &bh) in bar_heights.iter().enumerate() {
                let bx = start_x + idx as f32 * spacing;
                let b_rect = Rect::from_min_max(pos2(bx, base_y - bh), pos2(bx + spacing * 0.6, base_y));
                painter.rect_filled(b_rect, CornerRadius::same(3), bar_colors[idx]);
            }
        }
        "@originui/donut-chart" => {
            let cw = (rect.width() - 20.0).clamp(150.0, 210.0);
            let c_rect = Rect::from_center_size(center, vec2(cw, 44.0));
            let fill = if dark { Color32::from_rgb(28, 30, 36) } else { Color32::WHITE };
            painter.rect_filled(c_rect, CornerRadius::same(6), fill);
            painter.rect_stroke(c_rect, CornerRadius::same(6), Stroke::new(1.0, Color32::from_rgb(226, 232, 240)), StrokeKind::Inside);
            let circle_center = pos2(c_rect.min.x + 28.0, c_rect.center().y);
            painter.circle_stroke(circle_center, 13.0, Stroke::new(4.5, Color32::from_rgb(99, 102, 241)));
            painter.text(
                circle_center,
                Align2::CENTER_CENTER,
                "68%",
                semibold(8.5),
                if dark { Color32::WHITE } else { Color32::from_rgb(15, 23, 42) },
            );
            let l1_pos = pos2(c_rect.min.x + 50.0, c_rect.min.y + 9.0);
            painter.text(l1_pos, Align2::LEFT_TOP, "● Direct: 68%", semibold(9.5), Color32::from_rgb(79, 70, 229));
            let l2_pos = pos2(c_rect.min.x + 50.0, c_rect.min.y + 23.0);
            painter.text(l2_pos, Align2::LEFT_TOP, "● Organic: 32%", semibold(9.5), Color32::from_rgb(16, 185, 129));
        }
        "@kibo/kanban-board" => {
            let cw = (rect.width() - 16.0).clamp(160.0, 220.0);
            let col_w = (cw - 8.0) / 3.0;
            let start_x = center.x - cw / 2.0;
            let cols = [
                ("TODO", Color32::from_rgb(241, 245, 249), Color32::from_rgb(71, 85, 105), "Design"),
                ("IN PROG", Color32::from_rgb(254, 243, 199), Color32::from_rgb(180, 83, 9), "Market"),
                ("DONE", Color32::from_rgb(209, 250, 229), Color32::from_rgb(4, 120, 87), "Registries"),
            ];
            for (idx, (hdr, col_fill, col_text, task)) in cols.iter().enumerate() {
                let col_rect = Rect::from_min_size(pos2(start_x + idx as f32 * (col_w + 4.0), center.y - 20.0), vec2(col_w, 40.0));
                painter.rect_filled(col_rect, CornerRadius::same(4), if dark { Color32::from_rgb(32, 35, 42) } else { *col_fill });
                painter.text(pos2(col_rect.center().x, col_rect.min.y + 4.0), Align2::CENTER_TOP, *hdr, semibold(8.0), *col_text);
                let task_rect = Rect::from_min_size(pos2(col_rect.min.x + 3.0, col_rect.min.y + 17.0), vec2(col_w - 6.0, 18.0));
                painter.rect_filled(task_rect, CornerRadius::same(3), if dark { Color32::from_rgb(24, 26, 32) } else { Color32::WHITE });
                painter.text(
                    task_rect.center(),
                    Align2::CENTER_CENTER,
                    *task,
                    regular(8.5),
                    if dark { Color32::WHITE } else { Color32::from_rgb(15, 23, 42) },
                );
            }
        }
        "@bundui/data-table" => {
            let cw = (rect.width() - 16.0).clamp(160.0, 220.0);
            let c_rect = Rect::from_center_size(center, vec2(cw, 42.0));
            let fill = if dark { Color32::from_rgb(28, 30, 36) } else { Color32::WHITE };
            painter.rect_filled(c_rect, CornerRadius::same(6), fill);
            painter.rect_stroke(c_rect, CornerRadius::same(6), Stroke::new(1.0, Color32::from_rgb(226, 232, 240)), StrokeKind::Inside);
            let hdr_rect = Rect::from_min_size(c_rect.min, vec2(cw, 14.0));
            painter.rect_filled(hdr_rect, CornerRadius::same(4), if dark { Color32::from_rgb(36, 39, 48) } else { Color32::from_rgb(248, 250, 252) });
            painter.text(
                pos2(hdr_rect.min.x + 6.0, hdr_rect.center().y),
                Align2::LEFT_CENTER,
                "NAME      ROLE      STATUS",
                semibold(7.5),
                Color32::from_rgb(100, 116, 139),
            );
            let r1_pos = pos2(c_rect.min.x + 6.0, c_rect.min.y + 22.0);
            painter.text(
                r1_pos,
                Align2::LEFT_CENTER,
                "Sarah C.   Lead UI",
                regular(8.5),
                if dark { Color32::WHITE } else { Color32::from_rgb(15, 23, 42) },
            );
            let badge1 = Rect::from_min_size(pos2(c_rect.max.x - 38.0, c_rect.min.y + 16.0), vec2(32.0, 12.0));
            painter.rect_filled(badge1, CornerRadius::same(3), Color32::from_rgb(209, 250, 229));
            painter.text(badge1.center(), Align2::CENTER_CENTER, "ACTIVE", semibold(7.0), Color32::from_rgb(6, 95, 70));
            let r2_pos = pos2(c_rect.min.x + 6.0, c_rect.min.y + 34.0);
            painter.text(
                r2_pos,
                Align2::LEFT_CENTER,
                "Alex R.    DevOps",
                regular(8.5),
                if dark { Color32::WHITE } else { Color32::from_rgb(15, 23, 42) },
            );
            let badge2 = Rect::from_min_size(pos2(c_rect.max.x - 38.0, c_rect.min.y + 28.0), vec2(32.0, 12.0));
            painter.rect_filled(badge2, CornerRadius::same(3), Color32::from_rgb(254, 243, 199));
            painter.text(badge2.center(), Align2::CENTER_CENTER, "PENDING", semibold(7.0), Color32::from_rgb(180, 83, 9));
        }
        "@animate-ui/marquee-ticker" => {
            let bw = (rect.width() - 20.0).clamp(160.0, 210.0);
            let b_rect = Rect::from_center_size(center, vec2(bw, 26.0));
            painter.rect_filled(b_rect, CornerRadius::same(13), Color32::from_rgb(23, 23, 23));
            painter.rect_stroke(b_rect, CornerRadius::same(13), Stroke::new(1.0, Color32::from_rgb(51, 51, 51)), StrokeKind::Inside);
            let tag_rect = Rect::from_min_size(pos2(b_rect.min.x + 4.0, b_rect.min.y + 4.0), vec2(38.0, 18.0));
            painter.rect_filled(tag_rect, CornerRadius::same(9), Color32::from_rgb(5, 150, 105));
            painter.text(tag_rect.center(), Align2::CENTER_CENTER, "⚡ FREE", semibold(8.0), Color32::WHITE);
            let text_pos = pos2(tag_rect.max.x + 6.0, b_rect.center().y);
            painter.text(text_pos, Align2::LEFT_CENTER, "✦ Shadcn • Origin • Cult", regular(9.5), Color32::from_rgb(167, 243, 208));
        }
        "@animate-ui/spotlight-card" => {
            let cw = (rect.width() - 20.0).clamp(150.0, 210.0);
            let c_rect = Rect::from_center_size(center, vec2(cw, 44.0));
            painter.rect_filled(c_rect, CornerRadius::same(8), Color32::from_rgb(15, 23, 42));
            painter.rect_stroke(c_rect, CornerRadius::same(8), Stroke::new(1.0, Color32::from_rgb(51, 65, 85)), StrokeKind::Inside);
            painter.circle_filled(pos2(c_rect.center().x - 20.0, c_rect.center().y), 16.0, Color32::from_rgba_unmultiplied(2, 132, 199, 50));
            let t_pos = pos2(c_rect.min.x + 8.0, c_rect.min.y + 8.0);
            painter.text(t_pos, Align2::LEFT_TOP, "✦ Pointer Spotlight", semibold(10.5), Color32::WHITE);
            let sub_pos = pos2(c_rect.min.x + 8.0, c_rect.min.y + 24.0);
            painter.text(sub_pos, Align2::LEFT_TOP, "Dynamic illumination follows cursor", regular(8.5), Color32::from_rgb(148, 163, 184));
        }
        _ => {
            let c_rect = Rect::from_center_size(center, vec2(rect.width() - 24.0, 36.0));
            let fill = if dark { Color32::from_rgb(28, 30, 36) } else { Color32::WHITE };
            painter.rect_filled(c_rect, CornerRadius::same(6), fill);
            painter.rect_stroke(c_rect, CornerRadius::same(6), Stroke::new(1.0, Color32::from_rgb(226, 232, 240)), StrokeKind::Inside);
            painter.text(c_rect.center(), Align2::CENTER_CENTER, "Component Preview", semibold(11.0), default_text);
        }
    }
}
