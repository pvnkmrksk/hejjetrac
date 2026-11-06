//! Large arrays share one hardware-rendered surface instead of one swapchain
//! per rig. Session workers and numerical outputs remain independent.
use crate::{Desktop, session::Tool, tiling};
use eframe::egui;

pub fn required(active: usize) -> bool {
    active > 1
}

impl Desktop {
    pub(crate) fn reset_array(&mut self) -> Result<serde_json::Value, String> {
        if self
            .sessions
            .iter()
            .any(|s| !s.closed && (s.worker.is_none() || s.tool != Tool::Inspect))
        {
            return Err(
                "Start every rig and finish or cancel calibration before resetting all tracking"
                    .into(),
            );
        }
        let mut results = vec![];
        for s in self.sessions.iter_mut().filter(|s| !s.closed) {
            let result = crate::control::execute_session(s, "reset", &serde_json::json!({}));
            if let Err(error) = &result {
                s.error = error.clone();
            }
            results.push(
                serde_json::json!({"session":s.id,"queued":result.is_ok(),"error":result.err()}),
            );
        }
        Ok(
            serde_json::json!({"rigs":results,"completion":"Poll each rig's reset notice; source capture and its clock continue, tracking pose/path restart."}),
        )
    }
    pub(crate) fn set_array_view(&mut self, args: &serde_json::Value) -> Result<(), String> {
        if self
            .sessions
            .iter()
            .any(|s| !s.closed && s.show_calibration)
        {
            return Err("Finish or close calibration before changing all rig views".into());
        }
        for s in self.sessions.iter_mut().filter(|s| !s.closed) {
            if let Some(v) = args.get("full_image").and_then(serde_json::Value::as_bool) {
                s.full_image = v;
            }
            if let Some(v) = args.get("diagnostics").and_then(serde_json::Value::as_bool) {
                s.show_diagnostics = v;
            }
            if let Some(v) = args.get("overlays").and_then(serde_json::Value::as_bool) {
                s.show_overlays = v;
            }
        }
        Ok(())
    }
    pub(crate) fn record_array(
        &mut self,
        start: bool,
        directory: Option<&std::path::Path>,
    ) -> Result<serde_json::Value, String> {
        if start
            && self.sessions.iter().any(|s| {
                !s.closed
                    && (s.worker.is_none()
                        || s.snapshot.ended
                        || s.tool != Tool::Inspect
                        || s.snapshot.recording)
            })
        {
            return Err("Every rig must be running, outside calibration and not already recording before Record all".into());
        }
        if start && let Some(path) = directory {
            std::fs::create_dir(path).map_err(|e| {
                format!(
                    "Cannot create new array recording folder {}: {e}",
                    path.display()
                )
            })?;
        }
        let mut results = vec![];
        for s in self.sessions.iter_mut().filter(|s| !s.closed) {
            let result = if start {
                if let Some(path) = directory {
                    let args = serde_json::json!({"directory":path.join(format!("rig-{}",s.id)),"record_options":{"raw":s.record_raw,"debug":s.record_debug},"recording_tuning":{"encoding":s.recording_encoding}});
                    crate::control::execute_session(s, "record_start", &args).map(|_| ())
                } else {
                    s.queue_recording()
                }
            } else if s.worker.is_some() {
                s.worker
                    .as_ref()
                    .ok_or("Worker unavailable")?
                    .send(crate::runtime::Command::StopRecord)
            } else {
                Ok(())
            };
            if let Err(e) = &result {
                s.error = e.clone();
            }
            results.push(
                serde_json::json!({"session":s.id,"queued":result.is_ok(),"error":result.err()}),
            );
        }
        Ok(
            serde_json::json!({"rigs":results,"completion":"Poll sessions.recording and recording_finalizations; each rig reports failures independently."}),
        )
    }
    pub(crate) fn array_controls(&mut self, ui: &mut egui::Ui) {
        if crate::icons::toolbar_button(ui, crate::icons::Icon::Reset, false,
            "Reset tracking · all rigs\nRestart pose, heading and path; retain source time and integrated displacement.").clicked()
            && let Err(error) = self.reset_array()
        {
            self.startup_notice = error;
        }
        let active = self.sessions.iter().filter(|s| !s.closed).count();
        let recording = self
            .sessions
            .iter()
            .filter(|s| !s.closed && s.snapshot.recording)
            .count();
        let help = if recording > 0 {
            format!("Stop all recordings ({recording}/{active})")
        } else {
            format!("Record all {active} rigs")
        };
        if crate::icons::record_all(ui, recording > 0, &help).clicked()
            && let Err(e) = self.record_array(recording == 0, None)
        {
            self.startup_notice = e;
        }
        let enabled = !self
            .sessions
            .iter()
            .any(|s| !s.closed && s.show_calibration);
        ui.add_enabled_ui(enabled, |ui| {
            let full = self
                .sessions
                .iter()
                .filter(|s| !s.closed)
                .all(|s| s.full_image);
            let debug = self
                .sessions
                .iter()
                .filter(|s| !s.closed)
                .all(|s| s.show_diagnostics);
            let outlines = self
                .sessions
                .iter()
                .filter(|s| !s.closed)
                .all(|s| s.show_overlays);
            let controls = [
                (
                    crate::icons::Icon::Crop,
                    !full,
                    "full_image",
                    !full,
                    if full {
                        "Zoom to ball · all rigs"
                    } else {
                        "Show full frame · all rigs"
                    },
                ),
                (
                    crate::icons::Icon::Diagnostics,
                    debug,
                    "diagnostics",
                    !debug,
                    if debug {
                        "Show footage · all rigs"
                    } else {
                        "Show debug view · all rigs"
                    },
                ),
                (
                    crate::icons::Icon::Calibration,
                    outlines,
                    "overlays",
                    !outlines,
                    if outlines {
                        "Hide calibration outlines · all rigs"
                    } else {
                        "Show calibration outlines · all rigs"
                    },
                ),
            ];
            for (icon, active, key, value, help) in controls {
                if crate::icons::toolbar_button(ui, icon, active, help).clicked() {
                    let mut args = serde_json::json!({});
                    args[key] = serde_json::json!(value);
                    if let Err(e) = self.set_array_view(&args) {
                        self.startup_notice = e;
                    }
                }
            }
        });
    }

    pub(crate) fn ensure_dashboard(&mut self, ctx: &egui::Context) {
        if !self.dashboard && required(self.sessions.iter().filter(|s| !s.closed).count()) {
            self.dashboard = true;
            self.dashboard_columns = self
                .args
                .grid
                .map(|p| p.0 as usize)
                .or(self.workspace.columns);
            self.root_hidden = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Title("HejjéTrac · Rig grid".into()));
            if let Some(size) = self
                .args
                .window_size
                .map(|p| [p.0, p.1])
                .or(self.workspace.grid_window_size)
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                    size[0], size[1],
                )));
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
            }
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
            session.tile_settings_rect = None;
            session.tile_reset_rect = None;
        }
        if self.dashboard_focus.is_none() {
            egui::TopBottomPanel::top("grid-actions").show(ctx, |ui| {
                ui.spacing_mut().interact_size.y = 38.0;
                ui.horizontal_wrapped(|ui| {
                    if crate::icons::toolbar_button(
                        ui, crate::icons::Icon::Rigs, false,
                        "Rig grid settings\nArrange rigs, manage configurations and shared controls.",
                    ).clicked()
                        && let Some(session) = self.sessions.iter_mut().find(|s| !s.closed)
                    {
                        session.show_rigs = true;
                    }
                    self.array_controls(ui);
                    ui.small(format!(
                        "{} rigs",
                        self.sessions.iter().filter(|s| !s.closed).count()
                    ));
                });
            });
        }
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
                let focused = self.dashboard_focus.is_some();
                let (columns, rows) = if focused {
                    (Some(1), None)
                } else if self
                    .dashboard_columns
                    .zip(self.workspace.rows)
                    .is_some_and(|(c, r)| c * r < active.len())
                {
                    (None, None)
                } else {
                    (self.dashboard_columns, self.workspace.rows)
                };
                let tiles =
                    tiling::grid(ui.available_rect_before_wrap(), active.len(), columns, rows);
                if tiles.is_empty() && !active.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label("Enlarge the window or adjust the grid layout in Rig Manager.");
                    });
                }
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
        let document = hejjetrac_io::ConfigDocument::parse("src_fn :\n".into())?;
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
            mcp: None,
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
        let mut doc = hejjetrac_io::ConfigDocument::parse(std::fs::read_to_string(
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
