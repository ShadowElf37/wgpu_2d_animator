use egui;
use crate::colormap::Colormap;

/// Build the egui overlay: colorbar on the right, axis tick labels around the data.
pub fn build(ctx: &egui::Context, vmin: f32, vmax: f32, colormap: Colormap) {
    ctx.set_visuals(egui::Visuals::dark());

    // ── Colorbar (right panel) ────────────────────────────────────────────────
    egui::SidePanel::right("colorbar")
        .resizable(false)
        .exact_width(84.0)
        .frame(
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(13, 13, 31))
                .inner_margin(egui::Margin::symmetric(6.0, 8.0)),
        )
        .show(ctx, |ui| {
            draw_colorbar(ui, vmin, vmax, colormap);
        });

    // ── Axis tick labels (central panel, no background) ───────────────────────
    egui::CentralPanel::default()
        .frame(egui::Frame::none())
        .show(ctx, |ui| {
            draw_axis_ticks(ui.painter(), ui.max_rect());
        });
}

// ─────────────────────────────────────────────────────────────────────────────
// Colorbar
// ─────────────────────────────────────────────────────────────────────────────

fn draw_colorbar(ui: &mut egui::Ui, vmin: f32, vmax: f32, colormap: Colormap) {
    let lut     = colormap.lut_rgba8();
    let painter = ui.painter();
    let avail   = ui.max_rect();

    let bar_w  = 16.0_f32;
    let bar_rect = egui::Rect::from_min_max(
        egui::pos2(avail.left(),          avail.top()),
        egui::pos2(avail.left() + bar_w, avail.bottom()),
    );
    let tick_x0 = bar_rect.right();
    let label_x = tick_x0 + 6.0;

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
            0.0, col,
        );
    }

    // Border.
    painter.rect_stroke(
        bar_rect, 0.0,
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
            egui::Align2::LEFT_CENTER,
            format!("{:.3}", value),
            egui::FontId::monospace(10.0),
            egui::Color32::from_gray(220),
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Axis ticks
// ─────────────────────────────────────────────────────────────────────────────
// Painted transparently over the data in the central panel.
// The data coordinate system is [0,1]×[0,1]; Y=0 is the top row.

fn draw_axis_ticks(painter: &egui::Painter, rect: egui::Rect) {
    let col  = egui::Color32::from_rgba_unmultiplied(220, 220, 220, 180);
    let font = egui::FontId::monospace(10.0);
    let n    = 5;

    for i in 0..=n {
        let t     = i as f32 / n as f32;
        let label = format!("{:.2}", t);

        // X-axis ticks along the bottom edge (value increases left → right).
        let x = rect.left() + t * rect.width();
        painter.line_segment(
            [egui::pos2(x, rect.bottom()), egui::pos2(x, rect.bottom() - 5.0)],
            egui::Stroke::new(1.0, col),
        );
        painter.text(
            egui::pos2(x, rect.bottom() - 7.0),
            egui::Align2::CENTER_BOTTOM,
            &label,
            font.clone(),
            col,
        );

        // Y-axis ticks along the left edge (value increases bottom → top, so flip).
        let y = rect.bottom() - t * rect.height();
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.left() + 5.0, y)],
            egui::Stroke::new(1.0, col),
        );
        painter.text(
            egui::pos2(rect.left() + 7.0, y),
            egui::Align2::LEFT_CENTER,
            &label,
            font.clone(),
            col,
        );
    }
}
