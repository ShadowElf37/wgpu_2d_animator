use egui;
use crate::colormap::Colormap;

/// Build the egui overlay.
///
/// `zoom` and `pan` are updated in-place from mouse scroll / drag events that
/// occur inside the central panel.  The caller must copy them out before the
/// `egui::Context::run` closure and write them back after, to avoid a borrow
/// conflict with the context.
pub fn build(
    ctx:      &egui::Context,
    vmin:     f32,
    vmax:     f32,
    colormap: Colormap,
    zoom:     &mut f32,
    pan:      &mut [f32; 2],
    title:    Option<&str>,
    bare:     bool,
) {
    ctx.set_visuals(egui::Visuals::dark());

    let screen_rect = ctx.screen_rect();

    // ── Central panel — full-window, no background ────────────────────────────
    egui::CentralPanel::default()
        .frame(egui::Frame::none())
        .show(ctx, |ui| {
            let panel_rect = ui.max_rect();
            let response   = ui.allocate_rect(panel_rect, egui::Sense::drag());

            // Scroll → zoom toward cursor.
            let scroll = if response.hovered() {
                ctx.input(|i| i.smooth_scroll_delta.y)
            } else {
                0.0
            };
            if scroll != 0.0 {
                let factor   = (scroll * 0.003_f32).exp();
                let new_zoom = (*zoom * factor).clamp(0.05, 200.0);
                if let Some(mp) = response.hover_pos() {
                    let mx = mp.x / screen_rect.width();
                    let my = mp.y / screen_rect.height();
                    let dx = (mx - 0.5 - pan[0]) / *zoom + 0.5;
                    let dy = (my - 0.5 - pan[1]) / *zoom + 0.5;
                    pan[0] = mx - 0.5 - (dx - 0.5) * new_zoom;
                    pan[1] = my - 0.5 - (dy - 0.5) * new_zoom;
                }
                *zoom = new_zoom;
            }

            // Drag → pan.
            if response.dragged() {
                let d  = response.drag_delta();
                pan[0] += d.x / screen_rect.width();
                pan[1] += d.y / screen_rect.height();
            }

            let painter = ui.painter();
            if !bare {
                draw_axis_ticks(painter, panel_rect, screen_rect, *zoom, pan);
                draw_colorbar(painter, panel_rect, vmin, vmax, colormap);
            }
            if let Some(t) = title {
                draw_title(painter, panel_rect, t);
            }
        });
}

// ─────────────────────────────────────────────────────────────────────────────
// Title label  (top-left corner, semi-transparent)
// ─────────────────────────────────────────────────────────────────────────────

fn draw_title(painter: &egui::Painter, panel: egui::Rect, title: &str) {
    let pos = egui::pos2(panel.left() + 10.0, panel.top() + 8.0);
    // Shadow for legibility on any background.
    painter.text(
        egui::pos2(pos.x + 1.0, pos.y + 1.0),
        egui::Align2::LEFT_TOP,
        title,
        egui::FontId::proportional(15.0),
        egui::Color32::from_black_alpha(160),
    );
    painter.text(
        pos,
        egui::Align2::LEFT_TOP,
        title,
        egui::FontId::proportional(15.0),
        egui::Color32::from_rgba_unmultiplied(240, 240, 240, 220),
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Colorbar  (painted directly — no container, no background)
// ─────────────────────────────────────────────────────────────────────────────

fn draw_colorbar(
    painter:  &egui::Painter,
    panel:    egui::Rect,
    vmin:     f32,
    vmax:     f32,
    colormap: Colormap,
) {
    let lut = colormap.lut_rgba8();

    let bar_w    = 16.0_f32;
    let margin_r =  8.0_f32;
    let margin_v = 16.0_f32;

    let bar_rect = egui::Rect::from_min_max(
        egui::pos2(panel.right() - margin_r - bar_w, panel.top()    + margin_v),
        egui::pos2(panel.right() - margin_r,          panel.bottom() - margin_v),
    );
    let tick_x0 = bar_rect.left() - 4.0;
    let label_x = tick_x0 - 2.0;

    // Gradient — 64 strips (bottom = vmin, top = vmax).
    let n = 64usize;
    for i in 0..n {
        let t0  = i as f32 / n as f32;
        let t1  = (i + 1) as f32 / n as f32;
        let y0  = bar_rect.bottom() - t0 * bar_rect.height();
        let y1  = bar_rect.bottom() - t1 * bar_rect.height();
        let idx = ((t0 * 255.0) as usize).min(255) * 4;
        let col = egui::Color32::from_rgb(lut[idx], lut[idx + 1], lut[idx + 2]);
        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(bar_rect.left(), y1), egui::pos2(bar_rect.right(), y0)),
            0.0,
            col,
        );
    }

    // Border.
    painter.rect_stroke(
        bar_rect,
        0.0,
        egui::Stroke::new(1.0, egui::Color32::from_gray(90)),
    );

    // Five ticks at 0 / 25 / 50 / 75 / 100 %.
    for i in 0..=4 {
        let t     = i as f32 / 4.0;
        let value = vmin + t * (vmax - vmin);
        let y     = bar_rect.bottom() - t * bar_rect.height();
        painter.line_segment(
            [egui::pos2(tick_x0, y), egui::pos2(tick_x0 + 4.0, y)],
            egui::Stroke::new(1.0, egui::Color32::from_gray(200)),
        );
        painter.text(
            egui::pos2(label_x, y),
            egui::Align2::RIGHT_CENTER,
            format!("{:.3}", value),
            egui::FontId::monospace(10.0),
            egui::Color32::from_gray(220),
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Axis ticks
// ─────────────────────────────────────────────────────────────────────────────

fn draw_axis_ticks(
    painter:     &egui::Painter,
    rect:        egui::Rect,
    screen_rect: egui::Rect,
    zoom:        f32,
    pan:         &[f32; 2],
) {
    let col  = egui::Color32::from_rgba_unmultiplied(220, 220, 220, 180);
    let font = egui::FontId::monospace(10.0);
    let n    = 5usize;

    let prec = if zoom > 1.0 {
        (zoom.log10().ceil() as usize).saturating_add(2).min(6)
    } else {
        2
    };

    let sw = screen_rect.width();
    let sh = screen_rect.height();

    for i in 0..=n {
        let t = i as f32 / n as f32;

        // ── X axis (bottom edge) ──────────────────────────────────────────
        let x      = rect.left() + t * rect.width();
        let suv_x  = x / sw;
        let data_x = (suv_x - 0.5 - pan[0]) / zoom + 0.5;
        let label_x = format!("{:.prec$}", data_x, prec = prec);

        let align_x = if i == 0 {
            egui::Align2::LEFT_BOTTOM
        } else if i == n {
            egui::Align2::RIGHT_BOTTOM
        } else {
            egui::Align2::CENTER_BOTTOM
        };

        painter.line_segment(
            [egui::pos2(x, rect.bottom()), egui::pos2(x, rect.bottom() - 5.0)],
            egui::Stroke::new(1.0, col),
        );
        painter.text(
            egui::pos2(x, rect.bottom() - 7.0),
            align_x,
            &label_x,
            font.clone(),
            col,
        );

        // ── Y axis (left edge, increasing bottom → top) ───────────────────
        let y          = rect.bottom() - t * rect.height();
        let suv_y      = y / sh;
        let data_uv_y  = (suv_y - 0.5 - pan[1]) / zoom + 0.5;
        let physical_y = 1.0 - data_uv_y;
        let label_y    = format!("{:.prec$}", physical_y, prec = prec);

        let align_y = if i == 0 {
            egui::Align2::LEFT_TOP
        } else if i == n {
            egui::Align2::LEFT_BOTTOM
        } else {
            egui::Align2::LEFT_CENTER
        };

        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.left() + 5.0, y)],
            egui::Stroke::new(1.0, col),
        );
        painter.text(
            egui::pos2(rect.left() + 7.0, y),
            align_y,
            &label_y,
            font.clone(),
            col,
        );
    }
}
