//! The Icons tab: searchable Font Awesome Free (v6) vector icon library in the left sidebar panel.

use egui::{Align2, CornerRadius, Id, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::json;

use super::fa6_icons::{FA6_ICONS, FaIconDef};
use crate::FigApp;
use crate::a11y::Tip;
use crate::theme::{Tokens, regular};

const ICON_SIZE: f32 = 38.0;
const ICON_GAP: f32 = 6.0;

/// Draw the real Font Awesome Free (v6) vector icon using its SVG path geometry.
fn paint_fa_icon(painter: &egui::Painter, rect: Rect, icon: &FaIconDef, color: egui::Color32, bg_color: egui::Color32) {
    let Ok(mut path) = figcraft_geom::BezPath::from_svg(icon.d) else { return };
    if (icon.width - 512.0).abs() > 1.0 {
        let offset_x = (512.0 - icon.width) / 2.0;
        path = figcraft_geom::Affine::translate((offset_x, 0.0)) * path;
    }

    let s = rect.width().min(rect.height()) / 512.0;
    let origin = rect.center() - vec2(512.0 * s / 2.0, 512.0 * s / 2.0);

    let mut subpaths: Vec<Vec<egui::Pos2>> = Vec::new();
    let mut current: Vec<egui::Pos2> = Vec::new();

    figcraft_geom::kurbo::flatten(path.iter(), 0.5, |el| match el {
        figcraft_geom::kurbo::PathEl::MoveTo(p) => {
            if current.len() >= 3 {
                subpaths.push(std::mem::take(&mut current));
            } else {
                current.clear();
            }
            current.push(pos2(origin.x + p.x as f32 * s, origin.y + p.y as f32 * s));
        }
        figcraft_geom::kurbo::PathEl::LineTo(p) => {
            let pt = pos2(origin.x + p.x as f32 * s, origin.y + p.y as f32 * s);
            if current.last().is_none_or(|last| (last.x - pt.x).hypot(last.y - pt.y) > 0.2) {
                current.push(pt);
            }
        }
        figcraft_geom::kurbo::PathEl::ClosePath => {
            if current.len() >= 3 {
                subpaths.push(std::mem::take(&mut current));
            } else {
                current.clear();
            }
        }
        _ => {}
    });
    if current.len() >= 3 {
        subpaths.push(current);
    }

    if subpaths.is_empty() {
        return;
    }

    // Sort subpaths by approximate area descending so outer contours are drawn before inner cutouts/holes
    subpaths.sort_by(|a, b| {
        let area_a = polygon_approx_area(a);
        let area_b = polygon_approx_area(b);
        area_b.partial_cmp(&area_a).unwrap_or(std::cmp::Ordering::Equal)
    });

    for i in 0..subpaths.len() {
        let poly = &subpaths[i];
        let test_pt = polygon_centroid(poly);
        let mut depth = 0;
        for other in &subpaths[..i] {
            if polygon_contains(other, test_pt) {
                depth += 1;
            }
        }
        let fill = if depth % 2 == 0 { color } else { bg_color };
        paint_poly(painter, poly, fill);
    }
}

fn polygon_approx_area(poly: &[egui::Pos2]) -> f32 {
    let mut area = 0.0f32;
    let n = poly.len();
    for i in 0..n {
        let j = (i + 1) % n;
        area += poly[i].x * poly[j].y - poly[j].x * poly[i].y;
    }
    area.abs() * 0.5
}

fn polygon_centroid(poly: &[egui::Pos2]) -> egui::Pos2 {
    if poly.is_empty() {
        return pos2(0.0, 0.0);
    }
    let sx: f32 = poly.iter().map(|p| p.x).sum();
    let sy: f32 = poly.iter().map(|p| p.y).sum();
    pos2(sx / poly.len() as f32, sy / poly.len() as f32)
}

fn polygon_contains(poly: &[egui::Pos2], p: egui::Pos2) -> bool {
    let mut inside = false;
    let n = poly.len();
    for i in 0..n {
        let j = (i + n - 1) % n;
        let (pi, pj) = (poly[i], poly[j]);
        let intersect = ((pi.y > p.y) != (pj.y > p.y))
            && (p.x < (pj.x - pi.x) * (p.y - pi.y) / (pj.y - pi.y) + pi.x);
        if intersect {
            inside = !inside;
        }
    }
    inside
}

fn paint_poly(painter: &egui::Painter, v: &[egui::Pos2], color: egui::Color32) {
    let n = v.len();
    if n < 3 {
        return;
    }
    let turn = |i: usize| {
        let (a, b, c) = (v[i], v[(i + 1) % n], v[(i + 2) % n]);
        (b - a).x * (c - b).y - (b - a).y * (c - b).x
    };
    let convex = (0..n).all(|i| turn(i) >= 0.0) || (0..n).all(|i| turn(i) <= 0.0);
    if convex {
        painter.add(egui::Shape::convex_polygon(v.to_vec(), color, egui::Stroke::NONE));
        return;
    }
    let indices = crate::icons::triangulate(v);
    if !indices.is_empty() {
        let mut mesh = egui::Mesh::default();
        for p in v {
            mesh.colored_vertex(*p, color);
        }
        mesh.indices = indices;
        painter.add(egui::Shape::mesh(mesh));
    }
    painter.add(egui::Shape::closed_line(v.to_vec(), egui::Stroke::new(0.5, color)));
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
        "Search Font Awesome Free (v6)...",
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
        .filter(|icon| {
            let matches_category = match selected_category {
                "Solid" => icon.cat == "Solid",
                "Regular" => icon.cat == "Regular",
                "Brands" => icon.cat == "Brands",
                _ => true,
            };
            let matches_query = filter.is_empty() || icon.name.to_lowercase().contains(&filter) || icon.id.contains(&filter);
            matches_category && matches_query
        })
        .collect();

    if matching.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(24.0);
            ui.label(egui::RichText::new("No Font Awesome icons found for this query").font(regular(12.0)).color(t.text_secondary));
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
                    let icon = matching[idx];
                    let (rect, resp) = ui.allocate_exact_size(vec2(ICON_SIZE, ICON_SIZE), Sense::click());
                    let hovered = resp.hovered();

                    if hovered {
                        ui.painter().rect_filled(rect, CornerRadius::same(6), t.hover);
                        ui.painter().rect_stroke(rect, CornerRadius::same(6), Stroke::new(1.0, t.border), StrokeKind::Inside);
                    }

                    let bg_color = if hovered { t.hover } else { t.panel };
                    let icon_color = if hovered { t.accent_text } else { t.icon };
                    paint_fa_icon(
                        ui.painter(),
                        Rect::from_center_size(rect.center(), vec2(20.0, 20.0)),
                        icon,
                        icon_color,
                        bg_color,
                    );

                    let clicked = resp.clicked();
                    resp.tip(format!("{} ({})", icon.name, icon.id));

                    if clicked {
                        insert_icon_to_canvas(app, icon);
                    }
                }
            });
            ui.add_space(ICON_GAP);
        }
    });
}

/// Helper to insert an icon as a real SVG vector node onto the current page canvas.
fn insert_icon_to_canvas(app: &mut FigApp, icon: &FaIconDef) {
    let Ok(ds) = app.session.ds() else { return };
    let (center_x, center_y) = (ds.view.cx, ds.view.cy);

    let svg_xml = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} 512\" width=\"32\" height=\"32\"><path fill=\"#1e1e1e\" d=\"{}\"/></svg>",
        icon.width, icon.d
    );
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(svg_xml.as_bytes());

    let _ = app.session.execute(
        "file.import",
        &json!({
            "name": format!("{}.svg", icon.name),
            "data": b64,
            "x": center_x,
            "y": center_y,
            "center": true,
        }),
    );
    app.toast(format!("Inserted {} ({})", icon.name, icon.id));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_subpaths() {
        let camera = FA6_ICONS.iter().find(|i| i.name == "camera").expect("camera icon exists");
        let path = figcraft_geom::BezPath::from_svg(camera.d).expect("valid SVG path");
        let s = 20.0 / 512.0;
        let origin = pos2(0.0, 0.0);
        let mut subpaths: Vec<Vec<egui::Pos2>> = Vec::new();
        let mut current: Vec<egui::Pos2> = Vec::new();
        figcraft_geom::kurbo::flatten(path.iter(), 0.5, |el| match el {
            figcraft_geom::kurbo::PathEl::MoveTo(p) => {
                if current.len() >= 3 {
                    subpaths.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
                current.push(pos2(origin.x + p.x as f32 * s, origin.y + p.y as f32 * s));
            }
            figcraft_geom::kurbo::PathEl::LineTo(p) => {
                let pt = pos2(origin.x + p.x as f32 * s, origin.y + p.y as f32 * s);
                if current.last().is_none_or(|last| (last.x - pt.x).hypot(last.y - pt.y) > 0.2) {
                    current.push(pt);
                }
            }
            figcraft_geom::kurbo::PathEl::ClosePath => {
                if current.len() >= 3 {
                    subpaths.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
            }
            _ => {}
        });
        if current.len() >= 3 {
            subpaths.push(current);
        }
        assert_eq!(subpaths.len(), 2);
        for sp in &subpaths {
            let tr = crate::icons::triangulate(sp);
            assert!(!tr.is_empty(), "triangulation must produce triangle indices");
        }
    }
}

