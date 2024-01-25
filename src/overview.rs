use crate::view::{DrawConfig, DrawingContext};
use crate::{wave_data::WaveData, State};
use eframe::egui::{Context, Frame, Sense, TopBottomPanel, Ui};
use eframe::emath::{Align2, RectTransform};
use eframe::epaint::{Pos2, Rect, Rounding, Vec2};

impl State {
    pub fn add_overview_panel(&self, ctx: &Context, waves: &WaveData) {
        TopBottomPanel::bottom("overview")
            .frame(Frame {
                fill: self.config.theme.primary_ui_color.background,
                ..Default::default()
            })
            .show(ctx, |ui| {
                self.draw_overview(ui, waves);
            });
    }

    fn draw_overview(&self, ui: &mut Ui, waves: &WaveData) {
        let (response, mut painter) = ui.allocate_painter(ui.available_size(), Sense::drag());
        let cfg = DrawConfig::new(response.rect.size().y);
        let container_rect = Rect::from_min_size(Pos2::ZERO, response.rect.size());
        let to_screen = RectTransform::from_to(container_rect, response.rect);

        let mut ctx = DrawingContext {
            painter: &mut painter,
            cfg: &cfg,
            // This 0.5 is very odd, but it fixes the lines we draw being smushed out across two
            // pixels, resulting in dimmer colors https://github.com/emilk/egui/issues/1322
            // 1 comes from subtracting .5 in cursor draw as y-adjusement is not required for known vertical lines.
            to_screen: &|x, y| to_screen.transform_pos(Pos2::new(x, y) + Vec2::new(0.5, 1.)),
            theme: &self.config.theme,
        };

        let viewport_all = waves.viewport_all();
        let minx =
            viewport_all.from_time_f64(waves.viewport.curr_left, response.rect.size().x as f64);
        let maxx =
            viewport_all.from_time_f64(waves.viewport.curr_right, response.rect.size().x as f64);
        let min = (ctx.to_screen)(minx as f32, 0.);
        let max = (ctx.to_screen)(maxx as f32, container_rect.max.y);
        ctx.painter.rect_filled(
            Rect { min, max },
            Rounding::ZERO,
            self.config
                .theme
                .canvas_colors
                .foreground
                .gamma_multiply(0.3),
        );
        waves.draw_cursor(
            &self.config.theme,
            &mut ctx,
            response.rect.size(),
            &viewport_all,
        );

        let mut ticks = self.get_ticks(
            &viewport_all,
            &waves.inner.metadata().timescale,
            response.rect.size().x,
            cfg.text_size,
        );

        if ticks.len() >= 2 {
            ticks.pop();
            ticks.remove(0);
            self.draw_ticks(
                None,
                &ticks,
                &ctx,
                response.rect.height() * 0.5,
                Align2::CENTER_CENTER,
            );
        }

        waves.draw_cursors(
            &self.config.theme,
            &mut ctx,
            response.rect.size(),
            &viewport_all,
        );

        waves.draw_cursor_number_boxes(
            &mut ctx,
            response.rect.size(),
            &self.config.theme,
            &viewport_all,
        );
    }
}
