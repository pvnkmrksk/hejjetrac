//! Large arrays share one hardware-rendered surface instead of one swapchain
//! per rig. Session workers and numerical outputs remain independent.
use crate::{Desktop, session::Tool, tiling};
use eframe::egui;

pub fn required(active: usize) -> bool {
    active > 4
}

impl Desktop {
    pub(crate) fn ensure_dashboard(&mut self, ctx: &egui::Context) {
        if !self.dashboard && required(self.sessions.iter().filter(|s| !s.closed).count()) {
            self.dashboard = true;
            self.dashboard_columns = self.args.grid.map(|p| p.0 as usize);
            self.root_hidden = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Title("HejjéTrac · Rig grid".into()));
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
        }
    }

    pub(crate) fn show_dashboard(&mut self, ctx: &egui::Context) {
        if let Some(session) = self
            .sessions
            .iter()
            .find(|s| !s.closed && s.show_calibration)
        {
            self.dashboard_focus = Some(session.id);
        }
        if self
            .dashboard_focus
            .is_some_and(|id| !self.sessions.iter().any(|s| s.id == id && !s.closed))
        {
            self.dashboard_focus = None;
        }
        // Poll all workers even while one rig is expanded. Returning to the grid
        // must immediately display their latest state, never queued old images.
        for session in self.sessions.iter_mut().filter(|s| !s.closed) {
            session.update();
            session.native_window = false;
            session.tile_rect = None;
            session.tile_open_rect = None;
        }
        egui::TopBottomPanel::top("grid-actions").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Rig grid");
                if ui.button("All rigs").clicked() {
                    self.dashboard_focus = None;
                    for session in &mut self.sessions {
                        if session.tool != Tool::Inspect {
                            session.cancel_calibration();
                            session.set_calibration_panel(false);
                        }
                    }
                }
                if ui.button("Rig manager…").clicked()
                    && let Some(session) = self.sessions.iter_mut().find(|s| !s.closed)
                {
                    session.show_rigs = true;
                }
                ui.small(format!(
                    "{} independent rigs",
                    self.sessions.iter().filter(|s| !s.closed).count()
                ));
            });
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                let active: Vec<_> = self
                    .sessions
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| !s.closed && self.dashboard_focus.is_none_or(|id| s.id == id))
                    .map(|(i, s)| (i, s.content_aspect()))
                    .collect();
                let aspects: Vec<_> = active.iter().map(|(_, a)| *a).collect();
                let focused = self.dashboard_focus.is_some();
                let tiles = tiling::plan(
                    ui.available_rect_before_wrap(),
                    &aspects,
                    egui::Vec2::ZERO,
                    if focused {
                        Some(1)
                    } else {
                        self.dashboard_columns
                    },
                );
                for ((index, _), tile) in active.into_iter().zip(tiles) {
                    let session = &mut self.sessions[index];
                    if session.show_tile(
                        ui,
                        egui::Rect::from_min_size(tile.position, tile.inner),
                        focused,
                    ) {
                        self.dashboard_focus = if focused { None } else { Some(session.id) };
                    }
                }
            });
        self.protect_grid_edits(ctx);
    }

    fn protect_grid_edits(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("grid-unsaved-close");
        let mut pending = ctx.data(|data| data.get_temp::<bool>(id).unwrap_or(false));
        if ctx.input(|i| i.viewport().close_requested())
            && self
                .sessions
                .iter()
                .any(|s| !s.closed && s.history.text != s.saved)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            pending = true;
        }
        if pending {
            egui::Modal::new(id).show(ctx, |ui| {
                ui.heading("Unsaved rig configurations");
                ui.label("Save configuration changes before closing all rigs?");
                for session in self
                    .sessions
                    .iter()
                    .filter(|s| !s.closed && s.history.text != s.saved)
                {
                    ui.label(session.display_name());
                    if !session.error.is_empty() {
                        ui.colored_label(ui.visuals().error_fg_color, &session.error);
                    }
                }
                ui.horizontal(|ui| {
                    let save = ui.button("Save all and close").clicked();
                    let discard = ui.button("Discard and close").clicked();
                    if ui.button("Cancel").clicked() {
                        pending = false;
                    }
                    if save || discard {
                        let mut ready = true;
                        if save {
                            for session in self
                                .sessions
                                .iter_mut()
                                .filter(|s| !s.closed && s.history.text != s.saved)
                            {
                                let result = session.save(false);
                                ready &= result.is_ok();
                                session.report(result);
                            }
                        }
                        if ready {
                            for session in &mut self.sessions {
                                session.closed = true;
                            }
                            pending = false;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                });
            });
        }
        ctx.data_mut(|data| data.insert_temp(id, pending));
    }
}

#[cfg(test)]
mod tests {
    use crate::{Args, Desktop, Pair, session::Session};
    use clap::Parser;
    use eframe::egui;

    #[test]
    fn sixteen_tiles_and_their_inspectors_share_the_root_surface()
    -> Result<(), Box<dyn std::error::Error>> {
        let document = fictrac_io::ConfigDocument::parse("src_fn :\n".into())?;
        let mut app = Desktop {
            sessions: (1..=16)
                .map(|id| Session::new(id, document.clone(), None))
                .collect(),
            dashboard: false,
            dashboard_focus: None,
            dashboard_columns: None,
            next_id: 17,
            args: Args::try_parse_from(["hejjetrac"])?,
            start: std::time::Instant::now(),
            requested_screenshot: false,
            last_state: std::time::Instant::now(),
            control: None,
            window_size: Pair(520., 480.),
            window_origin: Pair(0., 0.),
            tile_columns: 4,
            root_hidden: false,
            startup_notice: String::new(),
            startup_frames: 0,
            tile_array_requested: false,
            workspace: crate::state::Workspace::default(),
            tile_positions: std::collections::BTreeMap::new(),
        };
        app.sessions[15].show_settings = true;
        let ctx = egui::Context::default();
        // Native backends disable embedding: the grid must still never request
        // an extra viewport, including for settings or the rig manager.
        ctx.set_embed_viewports(false);
        let bounds = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1610., 986.));
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(bounds),
                ..Default::default()
            },
            |ctx| {
                app.ensure_dashboard(ctx);
                app.show_dashboard(ctx);
                app.sessions[0].show_rigs = true;
                app.rig_manager(ctx);
            },
        );
        assert!(app.dashboard);
        assert_eq!(output.viewport_output.len(), 1);
        for (i, session) in app.sessions.iter().enumerate() {
            assert!(!session.native_window);
            let r = session
                .tile_rect
                .ok_or("a rig is missing its visible tile")?;
            let rect = egui::Rect::from_min_size(egui::pos2(r[0], r[1]), egui::vec2(r[2], r[3]));
            assert!(bounds.contains_rect(rect));
            for previous in &app.sessions[..i] {
                let p = previous.tile_rect.ok_or("previous tile missing")?;
                assert!(!rect.intersects(egui::Rect::from_min_size(
                    egui::pos2(p[0], p[1]),
                    egui::vec2(p[2], p[3])
                )));
            }
        }
        // A native close event must not silently discard another rig's text.
        let draft = format!("{}# retained unsaved note\n", app.sessions[3].history.text);
        app.sessions[3].history.replace(draft.clone());
        let mut input = egui::RawInput {
            screen_rect: Some(bounds),
            ..Default::default()
        };
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .ok_or("root viewport missing")?
            .events
            .push(egui::ViewportEvent::Close);
        let guarded = ctx.run(input, |ctx| app.show_dashboard(ctx));
        assert!(
            guarded
                .viewport_output
                .get(&egui::ViewportId::ROOT)
                .ok_or("root output missing")?
                .commands
                .iter()
                .any(|c| matches!(c, egui::ViewportCommand::CancelClose))
        );
        assert!(app.sessions.iter().all(|s| !s.closed));
        assert_eq!(app.sessions[3].history.text, draft);
        // Exercise actual source ownership too: closing the first rig must
        // release its worker while another real-video worker keeps advancing.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut doc = fictrac_io::ConfigDocument::parse(std::fs::read_to_string(
            root.join("examples/upstream-config.txt"),
        )?)?;
        doc.set(
            "src_fn",
            &root.join("examples/sample.mp4").to_string_lossy(),
        )?;
        doc.set("sock_port", "0")?;
        doc.set("zmq_port", "0")?;
        for index in 0..2 {
            app.sessions[index] = Session::new(index as u64 + 1, doc.clone(), None);
            app.sessions[index].start()?;
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while app.sessions[..2].iter().any(|s| s.snapshot.frames < 3) {
            for s in &mut app.sessions[..2] {
                s.update();
            }
            if std::time::Instant::now() > deadline {
                return Err("real source startup timed out".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        app.sessions[0].closed = true;
        app.release_closed_grid_root();
        assert!(app.sessions[0].worker.is_none());
        let before = app.sessions[1].snapshot.frames;
        while app.sessions[1].snapshot.frames <= before {
            app.sessions[1].update();
            if std::time::Instant::now() > deadline {
                return Err("peer stopped after closing the first grid rig".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        Ok(())
    }
}
