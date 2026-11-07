#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod array;
mod atlas_cache;
mod calibration;
mod control;
mod dashboard;
#[cfg(any(target_os = "linux", test))]
mod graphics;
mod icons;
#[cfg(target_os = "linux")]
mod launch;
mod mcp;
mod profiles;
mod runtime;
mod session;
mod startup;
mod state;
mod tiling;
mod ui;
mod ui_metrics;
use clap::Parser;
use eframe::egui;
use hejjetrac_io::ConfigDocument;
use session::Session;
use std::{path::PathBuf, time::Instant};
#[derive(Parser)]
#[command(
    name = "hejjetrac",
    version,
    about = "Native spherical motion tracking, compatible with FicTrac"
)]
struct Args {
    /// Preview size in logical pixels, e.g. 480x360.
    #[arg(long, value_parser = parse_size)]
    window_size: Option<Pair>,
    /// Top-left window location in logical pixels, e.g. 0,24.
    #[arg(long, value_parser = parse_position, allow_hyphen_values = true)]
    window_position: Option<Pair>,
    /// Tile columns x rows, e.g. --windows 16 --grid 4x4.
    #[arg(long, value_parser = parse_grid)]
    grid: Option<Pair>,
    #[arg(short, long)]
    config: Option<PathBuf>,
    #[arg(long, default_value_t = 1)]
    windows: usize,
    /// Start saved rig siblings and report failures independently, without sample fallback.
    #[arg(long)]
    array: bool,
    #[arg(long)]
    screenshot: Option<PathBuf>,
    #[arg(long, default_value_t = 2.0)]
    screenshot_after: f64,
    #[arg(long)]
    exit_after: Option<f64>,
    #[arg(long)]
    camera_list: bool,
    #[arg(long)]
    probe_camera: Option<u32>,
    #[arg(long)]
    state_output: Option<PathBuf>,
    /// Accept agent JSONL commands on stdin and return structured responses on stdout.
    #[arg(long)]
    control_stdio: bool,
    /// Listen for authenticated desktop control on this IP:PORT (e.g. 0.0.0.0:17871 for LAN).
    /// Without this option, control accepts loopback connections only.
    #[arg(long, value_name = "IP:PORT")]
    control_bind: Option<std::net::SocketAddr>,
    /// Use Mesa software graphics on Linux when a native GPU driver cannot start.
    #[arg(long)]
    safe_mode: bool,
    /// Print Linux graphics environment and recent diagnostic-log locations, without opening a window.
    #[arg(long)]
    diagnostics: bool,
}
fn resource(relative: &str) -> Result<PathBuf, String> {
    let mut roots = vec![];
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        roots.push(parent.to_owned());
        roots.push(parent.join("../Resources"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }
    roots.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));
    roots
        .into_iter()
        .map(|p| p.join(relative))
        .find(|p| p.is_file())
        .ok_or_else(|| format!("Cannot find bundled {relative}; provide --config PATH"))
}
fn default_config() -> Result<PathBuf, String> {
    resource("examples/defaults/VR1.txt").or_else(|_| resource("VR1.txt"))
}
fn fallback_document(source: &ConfigDocument) -> Result<ConfigDocument, String> {
    let path = resource("examples/upstream-config.txt")?;
    let mut fallback =
        ConfigDocument::parse(std::fs::read_to_string(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    fallback
        .set(
            "src_fn",
            &resource("examples/sample.mp4")?.to_string_lossy(),
        )
        .map_err(|e| e.to_string())?;
    for (key, value) in source.fields().filter(|(key, _)| {
        key.starts_with("zmq_")
            || key.starts_with("sock_")
            || key.starts_with("com_")
            || matches!(
                *key,
                "rust_instance_name" | "output_fn" | "save_raw" | "save_debug" | "vid_codec"
            )
    }) {
        fallback.set(key, value).map_err(|e| e.to_string())?;
    }
    Ok(fallback)
}
#[derive(Clone, Copy, Debug)]
struct Pair(f32, f32);
fn parse_pair(value: &str, separator: char) -> Result<Pair, String> {
    let (x, y) = value.split_once(separator).ok_or("Expected two numbers")?;
    let pair = Pair(
        x.parse().map_err(|_| "Invalid first coordinate")?,
        y.parse().map_err(|_| "Invalid second coordinate")?,
    );
    if !pair.0.is_finite() || !pair.1.is_finite() {
        return Err("Coordinates must be finite".into());
    }
    Ok(pair)
}
fn parse_size(value: &str) -> Result<Pair, String> {
    let pair = parse_pair(value, 'x')?;
    if !(96.0..=8192.0).contains(&pair.0) || !(96.0..=8192.0).contains(&pair.1) {
        return Err("Window size must be 96x96 through 8192x8192".into());
    }
    Ok(pair)
}
fn parse_position(value: &str) -> Result<Pair, String> {
    let pair = parse_pair(value, ',')?;
    if pair.0.abs() > 1_000_000.0 || pair.1.abs() > 1_000_000.0 {
        return Err("Window position must be within ±1000000 logical pixels".into());
    }
    Ok(pair)
}
fn parse_grid(value: &str) -> Result<Pair, String> {
    let pair = parse_pair(value, 'x')?;
    if pair.0.fract() != 0.0
        || pair.1.fract() != 0.0
        || !(1.0..=16.0).contains(&pair.0)
        || !(1.0..=16.0).contains(&pair.1)
    {
        return Err("Grid dimensions must be integers 1..16".into());
    }
    Ok(pair)
}
struct Desktop {
    dashboard: bool,
    dashboard_focus: Option<u64>,
    dashboard_columns: Option<usize>,
    sessions: Vec<Session>,
    next_id: u64,
    args: Args,
    start: Instant,
    requested_screenshot: bool,
    last_state: Instant,
    control: Option<control::Bridge>,
    mcp: Option<mcp::Server>,
    window_size: Pair,
    window_origin: Pair,
    tile_columns: usize,
    root_hidden: bool,
    startup_notice: String,
    startup_frames: u8,
    tile_array_requested: bool,
    workspace: state::Workspace,
    tile_positions: std::collections::BTreeMap<u64, tiling::Tile>,
}
impl Desktop {
    fn camera_owners(&self, except: Option<u64>) -> std::collections::BTreeMap<u32, String> {
        self.sessions
            .iter()
            .filter(|session| {
                !session.closed && Some(session.id) != except && session.worker.is_some()
            })
            .filter_map(|session| {
                let snapshot = session.worker.as_ref()?.latest();
                if snapshot.ended {
                    return None;
                }
                snapshot
                    .actual_camera_id
                    .or_else(|| {
                        session.running_source.parse::<u32>().ok().and_then(|id| {
                            if let Some(binding) = session.running_camera_binding.as_deref() {
                                hejjetrac_camera::resolve_camera(&session.cameras, binding).ok()
                            } else {
                                Some(id)
                            }
                        })
                    })
                    .map(|id| {
                        (
                            id,
                            format!("{} (window {})", session.display_name(), session.id),
                        )
                    })
            })
            .collect()
    }
    fn refresh_camera_occupancy(&mut self) {
        let reservations = self
            .sessions
            .iter()
            .map(|session| (session.id, self.camera_owners(Some(session.id))))
            .collect::<Vec<_>>();
        for session in &mut self.sessions {
            if let Some((_, cameras)) = reservations.iter().find(|(id, _)| *id == session.id) {
                session.occupied_cameras = cameras.clone();
            }
        }
    }
    fn new(cc: &eframe::CreationContext<'_>, args: Args) -> Result<Self, String> {
        if let Some(render_state) = &cc.wgpu_render_state {
            let info = render_state.adapter.get_info();
            eprintln!(
                "Graphics: {} ({:?}, {:?})",
                info.name, info.backend, info.device_type
            );
            eprintln!("Graphics driver: {} · {}", info.driver, info.driver_info);
        }
        ui::configure_appearance(&cc.egui_ctx);
        let normal_startup = args.config.is_none() && !args.control_stdio;
        let workspace =
            state::workspace_path().and_then(|path| state::read::<state::Workspace>(&path, 65536));
        let mut workspace = match workspace {
            Ok(Some(value))
                if value.rigs.len() <= 16
                    && value.rigs.iter().all(|p| p.is_absolute())
                    && value.columns.is_none_or(|c| (1..=16).contains(&c))
                    && value.rows.is_none_or(|r| (1..=16).contains(&r))
                    && value.grid_window_size.is_none_or(state::valid_window_size) =>
            {
                value
            }
            Ok(None) => state::Workspace::default(),
            Ok(Some(_)) => {
                eprintln!("Saved workspace is invalid; using the default rig");
                state::Workspace::default()
            }
            Err(error) => {
                eprintln!("{error}");
                state::Workspace::default()
            }
        };
        if let Some(grid) = args.grid {
            workspace.columns = Some(grid.0 as usize);
            workspace.rows = Some(grid.1 as usize);
        }
        let restoring_workspace = normal_startup
            && args.windows == 1
            && !args.array
            && workspace.restore_on_launch
            && !workspace.rigs.is_empty();
        let mut explicit_session = None;
        let mut startup = if normal_startup {
            startup::default_document(&default_config()?)?
        } else {
            let path = args
                .config
                .clone()
                .map(Ok)
                .unwrap_or_else(|| resource("examples/upstream-config.txt"))?;
            let candidate = Session::from_config_path(1, path.clone())?;
            let document = match candidate.document() {
                Ok(document) => document,
                Err(_) => ConfigDocument::parse("src_fn:\n".into()).map_err(|e| e.to_string())?,
            };
            explicit_session = Some(candidate);
            startup::Startup {
                document,
                path,
                fresh: false,
                temporary: false,
                notice: String::new(),
            }
        };
        if restoring_workspace {
            let path = workspace.rigs[0].clone();
            let session = Session::from_config_path(1, path.clone())?;
            startup.path = path;
            startup.document = session
                .document()
                .unwrap_or(ConfigDocument::parse("src_fn:\n".into()).map_err(|e| e.to_string())?);
            startup.fresh = false;
            explicit_session = Some(session);
        }
        let path = startup.path;
        let doc = startup.document;
        let mut first =
            explicit_session.unwrap_or_else(|| Session::new(1, doc.clone(), Some(path.clone())));
        first.framebuffer_limit = cc
            .wgpu_render_state
            .as_ref()
            .map(|state| state.device.limits().max_texture_dimension_2d.min(4096))
            .unwrap_or(4096);
        first.max_window_inner_size =
            ui::framebuffer_window_limit(first.framebuffer_limit, cc.egui_ctx.pixels_per_point());
        cc.egui_ctx
            .send_viewport_cmd(egui::ViewportCommand::MaxInnerSize(
                first.max_window_inner_size,
            ));
        first.temporary_config = startup.temporary;
        first.fallback_notice = startup.notice.clone();
        if normal_startup
            && !restoring_workspace
            && args.windows == 1
            && !args.array
            && doc.camera_binding().map_err(|e| e.to_string())?.is_none()
        {
            first.startup_fallback = Some(fallback_document(&doc)?);
            if startup.fresh
                && let Some(camera) = first.cameras.first()
            {
                first.fields(&[("src_fn", camera.id.to_string())])?;
                first.changed = None;
            }
        }
        let result = if !first.error.is_empty() {
            Err(first.error.clone())
        } else if first.worker.is_some() {
            Ok(())
        } else {
            first.start()
        };
        if let Err(error) = &result {
            first.use_fallback(error);
        }
        if first.worker.is_none() {
            first.report(result);
        }
        if args.control_stdio {
            first.paused = true;
            first.send(runtime::Command::Pause(true));
        }
        let control = if args.control_stdio {
            Some(control::Bridge::start(cc.egui_ctx.clone())?)
        } else {
            None
        };
        let monitor = cc
            .egui_ctx
            .input(|i| i.viewport().monitor_size)
            .unwrap_or(egui::vec2(1440.0, 900.0));
        let grid = args
            .grid
            .unwrap_or(Pair((monitor.x / 520.0).floor().max(1.0), 1.0));
        let requested_size = args
            .window_size
            .or(first.saved_view.window_size.map(|s| Pair(s[0], s[1])))
            .unwrap_or_else(|| {
                if args.grid.is_some() {
                    Pair(
                        (monitor.x / grid.0 - 8.0).max(96.0),
                        ((monitor.y - 50.0) / grid.1 - 32.0).max(96.0),
                    )
                } else {
                    Pair(520.0, 480.0)
                }
            });
        let window_size = Pair(
            requested_size.0.min(first.max_window_inner_size.x),
            requested_size.1.min(first.max_window_inner_size.y),
        );
        if window_size.0 != requested_size.0 || window_size.1 != requested_size.1 {
            eprintln!(
                "Window dimensions limited to {}×{} logical pixels to respect the native framebuffer limit",
                window_size.0, window_size.1
            );
        }
        first.requested_window_size = Some(egui::vec2(window_size.0, window_size.1));
        let window_origin = args.window_position.unwrap_or(Pair(12.0, 36.0));
        cc.egui_ctx
            .send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                window_size.0,
                window_size.1,
            )));
        cc.egui_ctx
            .send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                window_origin.0,
                window_origin.1,
            )));
        let mut startup_notice = startup.notice;
        if let Ok(mode) = std::env::var("HEJJETRAC_STARTUP_MODE") {
            if !startup_notice.is_empty() {
                startup_notice.push_str(". ");
            }
            startup_notice.push_str(&format!("Compatible {mode} graphics startup is active. Use --diagnostics to locate the startup log."));
        }
        let arrange_on_start = args.grid.is_some();
        let mut app = Self {
            dashboard: false,
            dashboard_focus: None,
            dashboard_columns: None,
            sessions: vec![first],
            next_id: 2,
            args,
            start: Instant::now(),
            requested_screenshot: false,
            last_state: Instant::now() - std::time::Duration::from_secs(1),
            control,
            mcp: None,
            window_size,
            window_origin,
            tile_columns: grid.0 as usize,
            root_hidden: false,
            startup_notice,
            startup_frames: 0,
            tile_array_requested: arrange_on_start,
            workspace,
            tile_positions: std::collections::BTreeMap::new(),
        };
        for _ in 1..app.args.windows.clamp(1, 16) {
            app.spawn_from(app.sessions.len() - 1);
        }
        if app.args.array {
            app.start_saved_array(0);
        }
        if restoring_workspace {
            for path in app.workspace.rigs.clone().into_iter().skip(1) {
                app.spawn_saved(path);
            }
        }
        match mcp::Server::start(cc.egui_ctx.clone(), app.args.control_bind) {
            Ok(server) => app.mcp = Some(server),
            Err(e) => {
                eprintln!("{e}");
                app.startup_notice = e;
            }
        }
        Ok(app)
    }
    fn spawn_from(&mut self, index: usize) {
        if self
            .sessions
            .iter()
            .filter(|session| !session.closed)
            .count()
            >= 16
        {
            self.sessions[index].error = "At most 16 active instances".into();
            return;
        }
        // Saved arrays follow their file slots even when a prior file is invalid,
        // a camera is missing, or two configurations request the same camera.
        if let Some(anchor) = self.sessions[index].path.as_deref()
            && let Some(slot) = array::slot(anchor)
        {
            let candidate = (1..16)
                .filter_map(|offset| array::rig_path(anchor, (slot - 1 + offset) % 16 + 1))
                .find(|path| {
                    path.is_file()
                        && !self
                            .sessions
                            .iter()
                            .any(|s| !s.closed && s.path.as_ref() == Some(path))
                });
            if let Some(path) = candidate {
                self.spawn_saved(path);
                return;
            }
        }
        let document = if !self.sessions[index].fallback_notice.is_empty()
            && let Some(path) = &self.sessions[index].path
        {
            std::fs::read_to_string(path)
                .map_err(|e| e.to_string())
                .and_then(|text| ConfigDocument::parse(text).map_err(|e| e.to_string()))
        } else {
            self.sessions[index].document()
        };
        let result = document.and_then(|mut doc| {
            let mut path = self.sessions[index].path.clone();
            for _ in 0..16 {
                let (next, next_path) = doc
                    .next_instance(path.as_deref())
                    .map_err(|e| e.to_string())?;
                let used_path = next_path.as_ref().is_some_and(|p| {
                    self.sessions
                        .iter()
                        .any(|s| !s.closed && s.path.as_ref() == Some(p))
                });
                let camera = next.get("src_fn").and_then(|v| v.parse::<u32>().ok());
                let used_camera = camera.is_some_and(|id| {
                    self.sessions.iter().filter(|s| !s.closed).any(|s| {
                        s.document()
                            .ok()
                            .and_then(|d| d.get("src_fn").and_then(|v| v.parse::<u32>().ok()))
                            == Some(id)
                    })
                });
                if !used_path && !used_camera {
                    return Ok((next, next_path));
                }
                doc = next;
                path = next_path;
            }
            Err("No free instance/config slot in the next 16 sources".into())
        });
        match result {
            Ok((doc, path)) => {
                let anchor = self.sessions[index]
                    .path
                    .clone()
                    .or(self.sessions[index].source_base.clone());
                let mut next = Session::new(self.next_id, doc, path);
                next.framebuffer_limit = self.sessions[index].framebuffer_limit;
                next.max_window_inner_size = self.sessions[index].max_window_inner_size;
                next.occupied_cameras = self.camera_owners(None);
                if next.path.is_none() {
                    next.source_base = anchor;
                }
                self.next_id += 1;
                let result = next.start();
                if let Err(error) = &result {
                    next.use_fallback(error);
                }
                if next.worker.is_none() {
                    next.report(result);
                }
                self.sessions.push(next);
            }
            Err(e) => self.sessions[index].error = e,
        }
    }
    fn spawn_saved(&mut self, path: PathBuf) {
        match Session::from_config_path(self.next_id, path) {
            Ok(mut next) => {
                if let Some(first) = self.sessions.first() {
                    next.framebuffer_limit = first.framebuffer_limit;
                    next.max_window_inner_size = first.max_window_inner_size;
                }
                next.occupied_cameras = self.camera_owners(None);
                if next.error.is_empty() {
                    let result = next.start();
                    next.report(result);
                }
                self.next_id += 1;
                self.sessions.push(next);
            }
            Err(error) => {
                if let Some(first) = self.sessions.first_mut() {
                    first.error = error;
                }
            }
        }
    }
    fn start_saved_array(&mut self, index: usize) {
        self.tile_array_requested = true;
        let Some(anchor) = self.sessions[index].path.clone() else {
            self.sessions[index].error =
                "Save this rig as VR1.txt (through VR16.txt), then start the saved array".into();
            return;
        };
        for path in array::saved_paths(&anchor) {
            if !self
                .sessions
                .iter()
                .any(|s| !s.closed && s.path.as_ref() == Some(&path))
                && self.sessions.iter().filter(|s| !s.closed).count() < 16
            {
                self.spawn_saved(path);
            }
        }
        let owners: Vec<_> = self
            .sessions
            .iter()
            .map(|s| self.camera_owners(Some(s.id)))
            .collect();
        for (session, owners) in self
            .sessions
            .iter_mut()
            .zip(owners)
            .filter(|(s, _)| !s.closed)
        {
            session.startup_fallback = None;
            session.occupied_cameras = owners;
            if session.sample_fallback_active {
                session.worker = None;
                session.snapshot = runtime::Snapshot::default();
                session.textures.clear();
                if let Some(path) = session.path.clone() {
                    let result = session.load(path);
                    session.report(result);
                }
            } else if session.worker.is_none() || session.snapshot.ended {
                let result = if session.history.text == session.saved {
                    if let Some(path) = session.path.clone() {
                        session.load(path)
                    } else {
                        session.start()
                    }
                } else {
                    session.start()
                };
                session.report(result);
            } else if session.tool == session::Tool::Inspect {
                session.paused = false;
                session.send(runtime::Command::Pause(false));
            }
            session.show_rigs = true;
        }
    }

    fn rig_manager(&mut self, ctx: &egui::Context) {
        for session in &mut self.sessions {
            session.manager_reset_rect = None;
            session.manager_settings_rect = None;
        }
        if self.dashboard && self.dashboard_focus.is_some() {
            return;
        }
        if !self
            .sessions
            .iter()
            .any(|session| session.show_rigs && !session.closed)
        {
            return;
        }
        let mut close = false;
        let mut start_all = false;
        let mut open_four = false;
        let mut add_rigs = false;
        let mut new_rig = false;
        let mut save_workspace = false;
        let mut export_workspace = false;
        let mut load_workspace = false;
        let mut tile = false;
        let framebuffer_limit = self
            .sessions
            .first()
            .map(|s| s.framebuffer_limit)
            .unwrap_or(4096);
        let dashboard = self.dashboard;
        let mut contents = |ui: &mut egui::Ui| {
            let row_height = ui.spacing().interact_size.y;
            ui.spacing_mut().interact_size.y = 38.0;
            ui.horizontal_wrapped(|ui| {
                start_all =
                    icons::toolbar_button(ui, icons::Icon::Play, false, "Start all rigs").clicked();
                tile = icons::toolbar_button(ui, icons::Icon::Grid, false, "Arrange rig grid")
                    .clicked();
                self.array_controls(ui);
                icons::toolbar_menu(ui, "Rigs and workspace", |ui| {
                    open_four = ui.button("Open saved array").clicked();
                    export_workspace = ui.button("Save workspace copy…").clicked();
                    load_workspace = ui.button("Load workspace…").clicked();
                    add_rigs = ui.button("Add saved rigs…").clicked();
                    new_rig = ui
                        .add_enabled(
                            self.sessions.iter().filter(|s| !s.closed).count() < 16,
                            egui::Button::new("New rig"),
                        )
                        .clicked();
                    if open_four || add_rigs || new_rig || export_workspace || load_workspace {
                        ui.close();
                    }
                });
            });
            ui.spacing_mut().interact_size.y = row_height;
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.label("Layout");
                let mut columns = self.workspace.columns.unwrap_or(0);
                egui::ComboBox::from_id_salt("rig-grid-columns")
                    .selected_text(if columns == 0 {
                        "Auto square".into()
                    } else {
                        format!("{columns} columns")
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut columns, 0, "Auto square");
                        for c in 1..=16 {
                            ui.selectable_value(&mut columns, c, format!("{c} columns"));
                        }
                    });
                let count = self.sessions.iter().filter(|s| !s.closed).count();
                let mut rows = self.workspace.rows.unwrap_or(0);
                ui.label("Rows");
                ui.add(egui::DragValue::new(&mut rows).range(0..=16))
                    .on_hover_text("Rows · 0 chooses automatically");
                let proposed = (
                    (columns != 0).then_some(columns),
                    (rows != 0).then_some(rows),
                );
                if proposed != (self.workspace.columns, self.workspace.rows) {
                    if proposed
                        .0
                        .zip(proposed.1)
                        .is_some_and(|(c, r)| c * r < count)
                    {
                        self.startup_notice = "Grid needs at least one cell per open rig".into();
                    } else {
                        (self.workspace.columns, self.workspace.rows) = proposed;
                        self.args.grid = None;
                        tile = true;
                    }
                }
                let cols = self.workspace.columns.unwrap_or_else(|| {
                    self.workspace
                        .rows
                        .map(|r| count.div_ceil(r))
                        .unwrap_or_else(|| tiling::square_columns(count))
                });
                ui.small(format!(
                    "{} × {}",
                    cols,
                    self.workspace.rows.unwrap_or_else(|| count.div_ceil(cols))
                ));
            });
            if let Some(server) = &self.mcp {
                ui.horizontal_wrapped(|ui| {
                    ui.small(format!("Agent MCP: {}", server.url));
                    if ui.small_button("Copy connection JSON").clicked() {
                        ui.ctx().copy_text(server.connection.to_string());
                    }
                });
            }
            ui.checkbox(&mut self.workspace.restore_on_launch, "Restore on launch");
            save_workspace = ui.button("Save workspace").clicked();
            if !self.startup_notice.is_empty() {
                ui.colored_label(ui.visuals().warn_fg_color, &self.startup_notice);
            }
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for session in self.sessions.iter_mut().filter(|session| !session.closed) {
                    ui.push_id(session.id, |ui| {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.strong(session.display_name());
                                ui.label(session.launch_state());
                                session.rig_action_buttons(ui, true);
                            });
                            ui.small(session.compact_identity());
                            ui.small(session.hardware_port());
                            if let Some(outputs) = &session.snapshot.outputs {
                                if let Some(publisher) = &outputs.zmq {
                                    ui.small(format!(
                                        "ZMQ {} · {} · published {} · dropped {}",
                                        publisher.endpoint,
                                        if publisher.worker_alive && publisher.failures == 0 {
                                            "ready"
                                        } else {
                                            "failed"
                                        },
                                        publisher.published,
                                        publisher.dropped
                                    ));
                                }
                                if outputs.udp_enabled {
                                    ui.small(format!(
                                        "UDP socket {}",
                                        outputs
                                            .udp_destination
                                            .as_deref()
                                            .unwrap_or("destination unavailable")
                                    ));
                                }
                            }
                            if let Some(error) = &session.snapshot.command_error {
                                ui.colored_label(ui.visuals().error_fg_color, error);
                            }

                            let actual_source = if session.snapshot.source_label.is_empty() {
                                &session.running_source
                            } else {
                                &session.snapshot.source_label
                            };
                            ui.small(format!(
                                "{:.1} fps · {:.2} ms",
                                session.snapshot.actual_fps, session.snapshot.tracking_ms
                            ))
                            .on_hover_text(format!(
                                "{}\n{}\n{}",
                                actual_source,
                                session
                                    .running_camera_binding
                                    .as_deref()
                                    .unwrap_or("No hardware binding"),
                                session
                                    .path
                                    .as_ref()
                                    .map(|p| p.display().to_string())
                                    .unwrap_or_else(|| "Unsaved configuration".into())
                            ));
                            if !session.fallback_notice.is_empty() {
                                ui.small(&session.fallback_notice);
                            }
                            ui.collapsing("Full config", |ui| {
                                let mut text = session.history.text.clone();
                                ui.add(
                                    egui::TextEdit::multiline(&mut text)
                                        .code_editor()
                                        .desired_rows(10)
                                        .desired_width(f32::INFINITY)
                                        .interactive(false),
                                );
                                ui.small(
                                    "Edit and save this configuration from its Settings window.",
                                );
                            });
                            if !session.error.is_empty() {
                                ui.colored_label(ui.visuals().error_fg_color, &session.error);
                            }
                            if session.snapshot.ended && !session.snapshot.notice.is_empty() {
                                ui.colored_label(
                                    ui.visuals().error_fg_color,
                                    &session.snapshot.notice,
                                );
                            }
                        });
                    });
                }
                ui.add_space(8.0);
            });
        };
        if dashboard {
            let mut open = true;
            egui::Window::new("Rig manager")
                .id(egui::Id::new("grid-rig-manager"))
                .open(&mut open)
                .default_size([580.0, 540.0])
                .show(ctx, &mut contents);
            close = !open;
        } else {
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("rig-manager"),
                egui::ViewportBuilder::default()
                    .with_title("HejjéTrac · Rig manager")
                    .with_inner_size([580.0, 540.0])
                    .with_max_inner_size(ui::framebuffer_window_limit(
                        framebuffer_limit,
                        ctx.pixels_per_point(),
                    ))
                    .with_min_inner_size([400.0, 300.0]),
                |ctx, _| {
                    ui::constrain_framebuffer(ctx, framebuffer_limit);
                    close = ctx.input(|i| i.viewport().close_requested());
                    egui::CentralPanel::default().show(ctx, &mut contents);
                },
            );
        }
        if close {
            for session in &mut self.sessions {
                session.show_rigs = false;
            }
        }
        if start_all {
            for session in self.sessions.iter_mut().filter(|session| !session.closed) {
                if session.tool != session::Tool::Inspect {
                    session.error =
                        "Finish or cancel this rig's calibration before Start all".into();
                } else if session.worker.is_none() || session.snapshot.ended {
                    let result = session.start();
                    session.report(result);
                } else {
                    session.paused = false;
                    session.send(runtime::Command::Pause(false));
                }
            }
        }
        if open_four {
            if let Some(index) = self.sessions.iter().position(|session| !session.closed) {
                self.start_saved_array(index);
            }
            tile = true;
        }
        if add_rigs
            && let Some(paths) = rfd::FileDialog::new()
                .add_filter("Rig configurations", &["txt"])
                .pick_files()
        {
            for path in paths {
                if self.sessions.iter().filter(|s| !s.closed).count() >= 16 {
                    break;
                }
                if !self
                    .sessions
                    .iter()
                    .any(|s| !s.closed && s.path.as_ref() == Some(&path))
                {
                    self.spawn_saved(path);
                }
            }
            self.tile_array_requested = true;
        }
        if new_rig && let Some(index) = self.sessions.iter().position(|s| !s.closed) {
            self.spawn_from(index);
            self.tile_array_requested = true;
        }
        if save_workspace {
            match self.save_workspace() {
                Ok(()) => self.startup_notice = "Workspace saved.".into(),
                Err(error) => self.startup_notice = error,
            }
        }
        if export_workspace
            && let Some(path) = rfd::FileDialog::new()
                .set_title("Choose a parent folder for the workspace copy")
                .pick_folder()
        {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|v| v.as_millis())
                .unwrap_or(0);
            match self.export_workspace(&path.join(format!("Hejjetrac-workspace-{stamp}"))) {
                Ok(path) => {
                    self.startup_notice = format!(
                        "Workspace copy saved. Load {} from any build.",
                        path.display()
                    )
                }
                Err(error) => self.startup_notice = error,
            }
        }
        if load_workspace
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("HejjéTrac workspace", &["json"])
                .pick_file()
        {
            match self.load_workspace(&path) {
                Ok(()) => {
                    self.startup_notice = format!(
                        "Loaded {}. Save workspace to restore it on launch.",
                        path.display()
                    )
                }
                Err(error) => self.startup_notice = error,
            }
        }
        if tile {
            self.arrange(ctx, None);
        }
    }
    fn save_workspace(&mut self) -> Result<(), String> {
        let mut paths = vec![];
        for session in self.sessions.iter_mut().filter(|s| !s.closed) {
            let path = session
                .path
                .as_ref()
                .filter(|p| p.is_file())
                .ok_or("Save every rig's configuration before saving the startup workspace")?;
            if session.history.text != session.saved {
                return Err(
                    "Save the unsaved rig configurations first, then save the workspace".into(),
                );
            }
            paths.push(std::path::absolute(path).map_err(|e| e.to_string())?);
            session.save_view()?;
        }
        self.workspace.rigs = paths;
        state::write(&state::workspace_path()?, &self.workspace)
    }
    fn release_closed_grid_root(&mut self) {
        if self.sessions[0].closed && self.dashboard {
            // The dashboard still needs the native root surface, but its first
            // rig must release capture and outputs just like every other rig.
            let root = &mut self.sessions[0];
            root.worker = None;
            root.show_settings = false;
            root.snapshot = runtime::Snapshot::default();
            root.textures.clear();
            root.texture_image = None;
        }
    }
    fn arrange(&mut self, ctx: &egui::Context, columns: Option<usize>) {
        let columns = columns.or(self.workspace.columns);
        if self
            .sessions
            .iter()
            .any(|s| !s.closed && s.show_calibration)
        {
            self.startup_notice =
                "Finish or close calibration before arranging the tracking windows".into();
            return;
        }
        let count = self.sessions.iter().filter(|s| !s.closed).count();
        if columns
            .zip(self.workspace.rows)
            .is_some_and(|(c, r)| c * r < count)
        {
            self.startup_notice =
                "The selected grid has fewer cells than open rigs. Increase columns or rows."
                    .into();
            return;
        }
        self.ensure_dashboard(ctx);
        if self.dashboard {
            self.dashboard_columns = columns;
            self.dashboard_focus = None;
            return;
        }
        let monitor = ctx
            .input(|i| i.viewport().monitor_size)
            .unwrap_or(egui::vec2(1440.0, 900.0));
        let fallback_work = egui::Rect::from_min_max(
            egui::pos2(4.0, 32.0),
            egui::pos2(monitor.x - 4.0, monitor.y - 8.0),
        );
        let work = tiling::native_work_area(ctx).unwrap_or(fallback_work);
        let active: Vec<_> = self
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.closed)
            .collect();
        let aspects: Vec<_> = active.iter().map(|(_, s)| s.content_aspect()).collect();
        let decorations = active
            .iter()
            .filter_map(|(_, s)| s.window_rect.zip(s.window_inner_size))
            .map(|(outer, inner)| egui::vec2(outer[2] - inner[0], outer[3] - inner[1]))
            .fold(egui::vec2(0.0, 32.0), |a, b| a.max(b));
        let tiles = tiling::plan_layout(work, &aspects, decorations, columns, self.workspace.rows);
        if tiles.len() != active.len() {
            self.startup_notice = "This display cannot fit every window at its minimum size. Use fewer rigs on this display or a larger desktop.".into();
            return;
        }
        let placements: Vec<_> = active
            .into_iter()
            .zip(tiles)
            .map(|((index, session), tile)| (index, session.id, tile))
            .collect();
        for (index, session_id, tile) in placements {
            self.tile_positions.insert(session_id, tile);
            self.sessions[index].requested_window_size = Some(tile.inner);
            let id = if index == 0 {
                egui::ViewportId::ROOT
            } else {
                egui::ViewportId::from_hash_of(("hejjetrac", session_id))
            };
            ctx.send_viewport_cmd_to(id, egui::ViewportCommand::Maximized(false));
            ctx.send_viewport_cmd_to(id, egui::ViewportCommand::InnerSize(tile.inner));
            ctx.send_viewport_cmd_to(id, egui::ViewportCommand::OuterPosition(tile.position));
        }
    }
}
impl eframe::App for Desktop {
    fn on_exit(&mut self) {
        if let Err(error) = state::workspace_path().and_then(|p| state::write(&p, &self.workspace))
        {
            eprintln!("{error}");
        }
        for session in &mut self.sessions {
            if let Err(error) = session.save_view() {
                eprintln!("{error}");
            }
        }
    }
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let update_started = Instant::now();
        if self.startup_frames < 2 {
            self.startup_frames += 1;
            #[cfg(target_os = "linux")]
            if self.startup_frames == 2 {
                eprintln!("{}", launch::READY);
            }
            if self.startup_frames < 2 {
                ctx.request_repaint();
            }
        }
        self.refresh_camera_occupancy();
        if let Some(mut bridge) = self.control.take() {
            bridge.poll(self);
            self.control = Some(bridge);
        }
        if let Some(mut server) = self.mcp.take() {
            server.poll(self, ctx);
            self.mcp = Some(server);
        }
        self.ensure_dashboard(ctx);
        if self.dashboard {
            let size = ctx.content_rect().size();
            if state::valid_window_size([size.x, size.y]) {
                self.workspace.grid_window_size = Some([size.x, size.y]);
            }
            self.show_dashboard(ctx);
        } else {
            if !self.sessions[0].closed {
                self.sessions[0].show(ctx);
                // A delayed camera failure replaces its own notice. Keep the separate
                // bootstrap failure actionable until the preview has been saved.
                let first = &mut self.sessions[0];
                if !self.startup_notice.is_empty()
                    && !first.fallback_notice.contains(&self.startup_notice)
                {
                    if !first.fallback_notice.is_empty() {
                        first.fallback_notice.push_str(". ");
                    }
                    first.fallback_notice.push_str(&self.startup_notice);
                }
            }
            let mut cell = usize::from(!self.sessions[0].closed);
            for session in self.sessions.iter_mut().skip(1) {
                if session.closed {
                    continue;
                }
                let id = egui::ViewportId::from_hash_of(("hejjetrac", session.id));
                let tile = self
                    .tile_positions
                    .get(&session.id)
                    .copied()
                    .unwrap_or(tiling::Tile {
                        position: egui::pos2(
                            self.window_origin.0
                                + (cell % self.tile_columns) as f32 * (self.window_size.0 + 8.0),
                            self.window_origin.1
                                + (cell / self.tile_columns) as f32 * (self.window_size.1 + 32.0),
                        ),
                        inner: session
                            .requested_window_size
                            .unwrap_or(egui::vec2(self.window_size.0, self.window_size.1)),
                    });
                ctx.show_viewport_immediate(
                    id,
                    egui::ViewportBuilder::default()
                        .with_title(session.window_title())
                        .with_inner_size(tile.inner)
                        .with_position(tile.position)
                        .with_max_inner_size(session.max_window_inner_size)
                        .with_min_inner_size([96.0, 96.0]),
                    |ctx, class| {
                        session.native_window = class != egui::ViewportClass::Embedded;
                        session.show(ctx);
                    },
                );
                cell += 1;
            }
        }
        let requests: Vec<_> = self
            .sessions
            .iter_mut()
            .enumerate()
            .filter_map(|(i, s)| {
                let n = s.spawn_request;
                s.spawn_request = 0;
                (n > 0).then_some((i, n))
            })
            .collect();
        let array_requests: Vec<_> = self
            .sessions
            .iter_mut()
            .enumerate()
            .filter_map(|(i, s)| {
                let requested = s.start_array_request;
                s.start_array_request = false;
                requested.then_some(i)
            })
            .collect();
        for index in array_requests {
            self.start_saved_array(index);
        }
        for (mut i, requested) in requests {
            let n = if requested == 3 {
                4usize.saturating_sub(self.sessions.iter().filter(|s| !s.closed).count())
            } else {
                requested
            };
            for _ in 0..n {
                self.spawn_from(i);
                i = self.sessions.len() - 1;
            }
        }
        self.rig_manager(ctx);
        if self.tile_array_requested
            && self
                .sessions
                .iter()
                .filter(|s| !s.closed)
                .all(|s| s.snapshot.image.is_some() || s.snapshot.ended || !s.error.is_empty())
        {
            self.tile_array_requested = false;
            self.arrange(ctx, self.args.grid.map(|p| p.0 as usize));
        }
        if self.sessions.iter().all(|s| s.closed) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        self.release_closed_grid_root();
        if self.sessions[0].closed && !self.dashboard {
            // eframe owns a root event-loop window. Closing a tracking session
            // must not close other sessions: retain only a hidden event host,
            // and release this session's capture, outputs and image memory.
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if !self.root_hidden {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                let root = &mut self.sessions[0];
                root.worker = None;
                root.show_settings = false;
                root.snapshot = runtime::Snapshot::default();
                root.textures.clear();
                root.texture_image = None;
                self.root_hidden = true;
            }
        }
        let root = self.sessions[0].id;
        for session in self.sessions.iter_mut().filter(|s| s.closed) {
            if let Err(error) = session.save_view() {
                eprintln!("{error}");
            }
        }
        self.sessions.retain(|s| s.id == root || !s.closed);
        let elapsed = self.start.elapsed().as_secs_f64();
        if self.args.screenshot.is_some()
            && !self.requested_screenshot
            && elapsed > self.args.screenshot_after
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            self.requested_screenshot = true;
        }
        if let Some(path) = &self.args.screenshot {
            let screenshots: Vec<_> = ctx.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| {
                        if let egui::Event::Screenshot { image, .. } = e {
                            Some(image.clone())
                        } else {
                            None
                        }
                    })
                    .collect()
            });
            for screenshot in screenshots {
                let bytes: Vec<_> = screenshot
                    .pixels
                    .iter()
                    .flat_map(|c| c.to_array())
                    .collect();
                let result = image::save_buffer(
                    path,
                    &bytes,
                    screenshot.width() as u32,
                    screenshot.height() as u32,
                    image::ColorType::Rgba8,
                );
                if let Err(e) = result {
                    self.sessions[0].error = format!("screenshot: {e}");
                }
            }
        }
        if self.last_state.elapsed().as_millis() > 400
            && let Some(path) = &self.args.state_output
        {
            let instances:Vec<_>=self.sessions.iter().filter(|s| !s.closed).map(|s|serde_json::json!({"id":s.id,"frames":s.snapshot.frames,"source":s.source_edit,"config_path":s.path,"paused":s.snapshot.paused,"native_window":s.native_window,"tile_rect":s.tile_rect,"window_rect":s.window_rect,"window_inner_size":s.window_inner_size,"view":{"full_image":s.full_image,"diagnostics":s.show_diagnostics,"overlays":s.show_overlays,"display_fps":s.display_fps,"reuse_atlas":s.reuse_atlas},"window_title":s.window_title(),"revision":s.snapshot.revision,"notice":s.snapshot.notice,"error":s.error,"config_text":s.history.text,"ui":s.ui_metrics.value(),"preview_age_ms":s.snapshot.preview_age_ms})).collect();
            let value = serde_json::json!({"instances":instances,"elapsed_seconds":elapsed,"native_viewport_count":ctx.input(|i|i.raw.viewports.len()),"root_update_ms":update_started.elapsed().as_secs_f64()*1000.,"previous_native_frame_ms":frame.info().cpu_usage.map(|seconds|seconds*1000.)});
            match serde_json::to_vec_pretty(&value)
                .map_err(|e| e.to_string())
                .and_then(|data| std::fs::write(path, data).map_err(|e| e.to_string()))
            {
                Ok(()) => {}
                Err(e) => self.sessions[0].error = e,
            }
            self.last_state = Instant::now();
        }
        if self.args.exit_after.is_some_and(|t| elapsed > t) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        // Immediate child viewports repaint through their parent. A timer per
        // child makes their staggered wakeups redraw every window repeatedly.
        // Keep one timer on the root, including when its own rig is closed.
        if let Some(interval) = self
            .sessions
            .iter()
            .filter(|session| !session.closed)
            .map(Session::repaint_interval)
            .min()
        {
            ctx.request_repaint_after(interval);
        }
    }
}
fn app_icon() -> Result<egui::IconData, String> {
    let icon = image::load_from_memory(include_bytes!("../../../assets/hejjetrac.png"))
        .map_err(|e| format!("Bundled application icon: {e}"))?
        .into_rgba8();
    Ok(egui::IconData {
        width: icon.width(),
        height: icon.height(),
        rgba: icon.into_raw(),
    })
}
#[cfg(target_os = "linux")]
fn linux_use_x11(display: bool, requested: Option<&str>) -> Result<bool, String> {
    match requested {
        None => Ok(display),
        Some("x11") if display => Ok(true),
        Some("x11") => Err("WINIT_UNIX_BACKEND=x11 requires a nonempty DISPLAY; enable X11/XWayland or unset the override".into()),
        Some("wayland") => Ok(false),
        Some(other) => Err(format!("WINIT_UNIX_BACKEND={other:?} is unsupported; choose x11 or wayland, or unset it")),
    }
}

#[cfg(target_os = "linux")]
fn x11_bootstrap_needed(
    x11: bool,
    wayland_display: Option<&str>,
    wayland_socket: Option<&str>,
) -> bool {
    x11 && (wayland_display != Some("") || wayland_socket.is_some())
}

#[cfg(target_os = "linux")]
fn bootstrap_linux_display() -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    let requested = std::env::var("WINIT_UNIX_BACKEND").ok();
    let display = std::env::var("DISPLAY").is_ok_and(|value| !value.is_empty());
    let x11 = linux_use_x11(display, requested.as_deref())?;
    let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
    let wayland_socket = std::env::var("WAYLAND_SOCKET").ok();
    if x11_bootstrap_needed(x11, wayland_display.as_deref(), wayland_socket.as_deref()) {
        // wgpu's GLES instance probes Wayland independently of winit. Align
        // both before creating any windows or capture threads. Replacing this
        // same executable safely applies a private environment without unsafe
        // process-wide environment mutation or any external media runtime.
        // An empty display is intentional: an absent WAYLAND_DISPLAY can
        // connect to the default socket anyway. Remove WAYLAND_SOCKET rather
        // than giving that numeric inherited-FD variable an empty value.
        let executable = std::env::current_exe()
            .map_err(|error| format!("Cannot locate the application for X11 startup: {error}"))?;
        let error = std::process::Command::new(executable)
            .args(std::env::args_os().skip(1))
            .env("WAYLAND_DISPLAY", "")
            .env_remove("WAYLAND_SOCKET")
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .exec();
        return Err(format!(
            "Cannot start the compatible X11 graphics session: {error}"
        ));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn configure_graphics(mut options: eframe::NativeOptions) -> Result<eframe::NativeOptions, String> {
    let requested = std::env::var("WINIT_UNIX_BACKEND").ok();
    let display = std::env::var("DISPLAY").is_ok_and(|value| !value.is_empty());
    let x11 = linux_use_x11(display, requested.as_deref())?;
    if x11 {
        // Keep the tested X11/XWayland window path. Prefer compatible hardware
        // Vulkan: wgpu's Linux GLES presentation only offers blocking FIFO,
        // which throttles the whole UI when a child window becomes occluded.
        // Preserve explicit overrides and native Wayland-only installations.
        eprintln!("Linux window system: X11/XWayland");
        options.event_loop_builder = Some(Box::new(|builder| {
            use winit::platform::x11::EventLoopBuilderExtX11;
            builder.with_x11();
        }));
        graphics::configure_x11_presentation(&mut options.wgpu_options);
    } else if requested.as_deref() == Some("wayland") {
        options.event_loop_builder = Some(Box::new(|builder| {
            use winit::platform::wayland::EventLoopBuilderExtWayland;
            builder.with_wayland();
        }));
    }
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut options.wgpu_options.wgpu_setup {
        graphics::configure_setup(
            setup,
            wgpu::Backends::from_env(),
            x11,
            std::env::var("WGPU_ADAPTER_NAME").ok(),
            std::env::var("HEJJETRAC_STARTUP_MODE").is_ok_and(|mode| mode == "Software"),
        );
    }
    Ok(options)
}

#[cfg(not(target_os = "linux"))]
fn configure_graphics(options: eframe::NativeOptions) -> Result<eframe::NativeOptions, String> {
    Ok(options)
}

fn run() -> Result<(), String> {
    let args = Args::parse();
    if args.diagnostics {
        #[cfg(target_os = "linux")]
        {
            launch::print_diagnostics();
            return Ok(());
        }
        #[cfg(not(target_os = "linux"))]
        return Err("--diagnostics currently reports Linux startup details".into());
    }
    #[cfg(not(target_os = "linux"))]
    if args.safe_mode {
        return Err("--safe-mode currently applies to Linux graphics".into());
    }
    if !(1..=16).contains(&args.windows) {
        return Err("--windows must be 1..16".into());
    }
    if args
        .grid
        .is_some_and(|g| args.windows > (g.0 * g.1) as usize)
    {
        return Err("Grid has fewer cells than requested windows".into());
    }
    if args.camera_list || args.probe_camera.is_some() {
        for camera in hejjetrac_camera::list_cameras().map_err(|e| e.to_string())? {
            println!(
                "{}: {} · hardware ID: {} · USB socket ID: {}",
                camera.id,
                camera.name,
                camera.hardware_id.as_deref().unwrap_or("unavailable"),
                camera.port_id.as_deref().unwrap_or("unavailable")
            );
        }
        if let Some(id) = args.probe_camera {
            hejjetrac_camera::request_permission().map_err(|e| e.to_string())?;
            let mut camera = hejjetrac_camera::Camera::open(id, 30.0).map_err(|e| e.to_string())?;
            let start = Instant::now();
            let mut frames = 0;
            while frames < 30 && start.elapsed().as_secs() < 10 {
                if let Some(frame) = camera.next_frame().map_err(|e| e.to_string())? {
                    frames += 1;
                    if frames == 1 {
                        println!(
                            "capture {} × {} at {} ms",
                            frame.gray.width(),
                            frame.gray.height(),
                            frame.gray.timestamp_ms()
                        );
                    }
                }
            }
            if frames == 0 {
                return Err("No camera frames received within 10 seconds".into());
            }
            println!(
                "{frames} captured frames in {:.3} seconds",
                start.elapsed().as_secs_f64()
            );
        }
        return Ok(());
    }
    if args.state_output.as_ref().is_some_and(|p| p.exists()) {
        return Err("state output already exists".into());
    }
    if args.screenshot.as_ref().is_some_and(|p| p.exists()) {
        return Err("screenshot output already exists".into());
    }
    #[cfg(target_os = "linux")]
    bootstrap_linux_display()?;
    let options = configure_graphics(eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("hejjetrac")
            .with_title("HejjéTrac · VR1")
            .with_icon(app_icon()?)
            .with_inner_size([520.0, 480.0])
            .with_max_inner_size([4096.0, 4096.0])
            .with_min_inner_size([96.0, 96.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    })?;
    eframe::run_native(
        "HejjéTrac",
        options,
        Box::new(move |cc| {
            Desktop::new(cc, args)
                .map(|a| Box::new(a) as Box<dyn eframe::App>)
                .map_err(|e| e.into())
        }),
    )
    .map_err(|e| e.to_string())
}
fn main() -> std::process::ExitCode {
    #[cfg(target_os = "linux")]
    match launch::supervise() {
        Ok(Some(code)) => return code,
        Ok(None) => {}
        Err(error) => {
            eprintln!("hejjetrac: {error}");
            return std::process::ExitCode::FAILURE;
        }
    }
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("hejjetrac: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod arguments_tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_preview_prefers_x11_gles_and_preserves_explicit_backends() -> Result<(), String> {
        assert!(linux_use_x11(true, None)?);
        assert!(!linux_use_x11(false, None)?);
        assert!(!linux_use_x11(true, Some("wayland"))?);
        assert!(linux_use_x11(false, Some("x11")).is_err());
        assert!(linux_use_x11(true, Some("unknown")).is_err());
        assert!(x11_bootstrap_needed(true, Some("wayland-0"), None));
        assert!(x11_bootstrap_needed(true, None, None));
        assert!(!x11_bootstrap_needed(true, Some(""), None));
        assert!(x11_bootstrap_needed(true, Some(""), Some("")));
        assert!(!x11_bootstrap_needed(false, Some("wayland-0"), None));
        assert_eq!(
            graphics::linux_backends(None, true),
            wgpu::Backends::GL | wgpu::Backends::VULKAN
        );
        assert_eq!(
            graphics::linux_backends(Some(wgpu::Backends::VULKAN), true),
            wgpu::Backends::VULKAN
        );
        assert_eq!(
            graphics::linux_backends(None, false),
            wgpu::Backends::PRIMARY | wgpu::Backends::GL
        );
        Ok(())
    }

    #[test]
    fn tiny_preview_size_is_available_through_the_public_cli()
    -> Result<(), Box<dyn std::error::Error>> {
        let args = Args::try_parse_from(["hejjetrac", "--window-size", "96x96"])?;
        let size = args.window_size.ok_or("missing preview size")?;
        assert_eq!((size.0, size.1), (96.0, 96.0));
        for value in ["95x96", "96x95", "NaNx96", "96xinf", "8193x96"] {
            assert!(Args::try_parse_from(["hejjetrac", "--window-size", value]).is_err());
        }
        Ok(())
    }
}
