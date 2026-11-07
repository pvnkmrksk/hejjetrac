use crate::runtime::{Command, Snapshot, Worker};
use eframe::egui;
use hejjetrac_io::{ConfigDocument, DocumentHistory, SourceSpec};
use std::{fs, path::PathBuf, time::Instant};
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Inspect,
    Rim,
    Exclude,
    Square,
    Edit,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    Source,
    Calibration,
    Display,
    Atlas,
    Recording,
    Outputs,
    Advanced,
}

pub struct Session {
    pub ui_metrics: crate::ui_metrics::UiMetrics,
    pub source_base: Option<PathBuf>,
    pub running_source: String,
    pub running_camera_binding: Option<String>,
    pub camera_binding_draft: Option<String>,
    pub running_fps: f64,
    pub id: u64,
    pub history: DocumentHistory,
    pub path: Option<PathBuf>,
    pub saved: String,
    pub disk_conflict: bool,
    pub worker: Option<Worker>,
    pub snapshot: Snapshot,
    pub error: String,
    pub source_edit: String,
    pub show_settings: bool,
    pub show_calibration: bool,
    pub calibration_previous_view: Option<(bool, bool)>,
    pub show_config_text: bool,
    pub config_text_editing: bool,
    pub settings_section: SettingsSection,
    pub reuse_atlas: bool,
    pub saved_view: crate::state::ViewPreferences,
    pub view_state_error: String,
    pub show_about: bool,
    pub show_rigs: bool,
    pub show_diagnostics: bool,
    pub show_overlays: bool,
    pub display_fps: u32,
    pub cameras: Vec<hejjetrac_camera::CameraInfo>,
    pub occupied_cameras: std::collections::BTreeMap<u32, String>,
    pub camera_edit: Option<hejjetrac_camera::CameraFormatRequest>,
    pub camera_error: String,
    pub record_raw: bool,
    pub record_debug: bool,
    pub recording_encoding: hejjetrac_control::RecordingEncoding,
    pub startup_fallback: Option<ConfigDocument>,
    pub sample_fallback_active: bool,
    pub start_array_request: bool,
    pub fallback_notice: String,
    pub temporary_config: bool,
    pub paused: bool,
    pub realtime: bool,
    pub tool: Tool,
    pub edit_existing: bool,
    pub calibration_review: bool,
    pub calibration_redraw: bool,
    pub calibration_previous_paused: Option<bool>,
    pub magnifier: bool,
    pub points: Vec<[f64; 2]>,
    pub square_slots: Option<[Option<[f64; 2]>; 4]>,
    pub tile_open_rect: Option<[f32; 4]>,
    pub tile_settings_rect: Option<[f32; 4]>,
    pub tile_reset_rect: Option<[f32; 4]>,
    pub manager_settings_rect: Option<[f32; 4]>,
    pub manager_reset_rect: Option<[f32; 4]>,
    pub settings_position: Option<egui::Pos2>,
    pub tile_rect: Option<[f32; 4]>,
    pub window_rect: Option<[f32; 4]>,
    pub window_inner_size: Option<[f32; 2]>,
    pub preview_canvas_rect: Option<[f32; 4]>,
    pub preview_image_rect: Option<[f32; 4]>,
    pub framebuffer_limit: u32,
    pub max_window_inner_size: egui::Vec2,
    pub requested_window_size: Option<egui::Vec2>,
    pub calibration_preview_size: Option<egui::Vec2>,
    pub last_title: String,
    pub plane: String,
    pub full_image: bool,
    pub changed: Option<Instant>,
    pub revision: u64,
    pub textures: Vec<egui::TextureHandle>,
    pub texture_frame: u64,
    pub calibration_cache: std::cell::RefCell<Option<CalibrationCache>>,
    pub texture_image: Option<std::sync::Arc<crate::runtime::ViewFrame>>,
    pub spawn_request: usize,
    pub closed: bool,
    pub native_window: bool,
    pub close_pending: bool,
    pub last_disk_check: Instant,
    pub status: String,
    pub auto_apply: bool,
    pub pending_drag: Option<(usize, usize)>,
    pub recording_path: String,
    pub zmq_resume_port: Option<i32>,
    pub udp_resume_port: Option<i32>,
}
impl Session {
    pub fn new(id: u64, doc: ConfigDocument, path: Option<PathBuf>) -> Self {
        let text = doc.text().to_owned();
        let source_edit = doc.get("src_fn").unwrap_or("").to_owned();
        let mut session = Self {
            ui_metrics: crate::ui_metrics::UiMetrics::default(),
            source_base: path.clone(),
            running_source: String::new(),
            running_camera_binding: None,
            camera_binding_draft: None,
            running_fps: -1.0,
            id,
            history: DocumentHistory::new(text.clone()),
            path,
            saved: text,
            disk_conflict: false,
            worker: None,
            snapshot: Snapshot::default(),
            error: String::new(),
            source_edit,
            show_settings: false,
            show_calibration: false,
            calibration_previous_view: None,
            show_config_text: false,
            config_text_editing: false,
            settings_section: SettingsSection::Source,
            reuse_atlas: false,
            saved_view: crate::state::ViewPreferences::default(),
            view_state_error: String::new(),
            show_about: false,
            show_rigs: false,
            show_diagnostics: false,
            show_overlays: false,
            display_fps: 30,
            cameras: hejjetrac_camera::list_cameras().unwrap_or_default(),
            occupied_cameras: std::collections::BTreeMap::new(),
            camera_edit: None,
            camera_error: String::new(),
            zmq_resume_port: None,
            udp_resume_port: None,
            record_raw: hejjetrac_control::RecordOptions::from_document(&doc)
                .with_video()
                .raw,
            record_debug: doc.boolean("save_debug"),
            recording_encoding: hejjetrac_control::RecordingEncoding::Configured,
            startup_fallback: None,
            sample_fallback_active: false,
            start_array_request: false,
            fallback_notice: String::new(),
            temporary_config: false,
            paused: false,
            realtime: true,
            tool: Tool::Inspect,
            edit_existing: false,
            calibration_review: true,
            calibration_redraw: false,
            calibration_previous_paused: None,
            magnifier: true,
            points: vec![],
            square_slots: None,
            tile_open_rect: None,
            tile_settings_rect: None,
            tile_reset_rect: None,
            manager_settings_rect: None,
            manager_reset_rect: None,
            settings_position: None,
            tile_rect: None,
            window_rect: None,
            window_inner_size: None,
            preview_canvas_rect: None,
            preview_image_rect: None,
            framebuffer_limit: 4096,
            max_window_inner_size: egui::vec2(4096.0, 4096.0),
            requested_window_size: None,
            calibration_preview_size: None,
            last_title: String::new(),
            plane: "xy".into(),
            full_image: false,
            changed: None,
            revision: 0,
            textures: vec![],
            texture_frame: u64::MAX,
            calibration_cache: std::cell::RefCell::new(None),
            texture_image: None,
            spawn_request: 0,
            closed: false,
            native_window: true,
            close_pending: false,
            last_disk_check: Instant::now(),
            status: String::new(),
            auto_apply: true,
            pending_drag: None,
            recording_path: crate::runtime::recording_root()
                .join(format!("VR{id}"))
                .to_string_lossy()
                .into_owned(),
        };
        session.restore_view();
        session
    }
    pub(crate) fn view_preferences(&self) -> crate::state::ViewPreferences {
        let (full_image, overlays) = self
            .calibration_previous_view
            .unwrap_or((self.full_image, self.show_overlays));
        crate::state::ViewPreferences {
            full_image,
            diagnostics: self.show_diagnostics,
            overlays,
            magnifier: self.magnifier,
            display_fps: self.display_fps,
            reuse_atlas: self.reuse_atlas,
            window_size: if self.native_window {
                self.calibration_preview_size
                    .map(|v| [v.x, v.y])
                    .or(self.requested_window_size.map(|v| [v.x, v.y]))
                    .or(self.window_inner_size)
                    .or(self.saved_view.window_size)
            } else {
                self.saved_view.window_size
            },
        }
    }
    fn restore_view(&mut self) {
        let result = self
            .path
            .as_deref()
            .map(|path| {
                crate::state::read::<crate::state::ViewPreferences>(
                    &crate::state::sidecar(path, "view"),
                    4096,
                )
            })
            .transpose();
        match result {
            Ok(view) => {
                let view = view.flatten().unwrap_or_default();
                if !(1..=60).contains(&view.display_fps)
                    || view
                        .window_size
                        .is_some_and(|s| !crate::state::valid_window_size(s))
                {
                    self.view_state_error =
                        "Saved display FPS is invalid; using the default view".into();
                    return;
                }
                self.full_image = view.full_image;
                self.show_diagnostics = view.diagnostics;
                self.show_overlays = view.overlays;
                self.magnifier = view.magnifier;
                self.display_fps = view.display_fps;
                self.reuse_atlas = view.reuse_atlas;
                self.requested_window_size = view.window_size.map(|s| egui::vec2(s[0], s[1]));
                self.saved_view = view;
            }
            Err(error) => self.view_state_error = error,
        }
    }
    pub fn save_view(&mut self) -> Result<(), String> {
        let view = self.view_preferences();
        if view != self.saved_view
            && let Some(path) = &self.path
        {
            crate::state::write(&crate::state::sidecar(path, "view"), &view)?;
            self.saved_view = view;
        }
        Ok(())
    }
    pub fn atlas_cache_path(&self) -> Option<PathBuf> {
        self.path
            .as_deref()
            .map(|path| crate::state::sidecar(path, "atlas"))
    }
    pub fn set_reuse_atlas(&mut self, enabled: bool) -> Result<(), String> {
        if enabled {
            let document = self.document()?;
            if document
                .get("src_fn")
                .is_some_and(|s| s.parse::<u32>().is_ok())
                && document
                    .camera_binding()
                    .map_err(|e| e.to_string())?
                    .is_none()
            {
                return Err(
                    "Link this rig to its physical camera before enabling automatic atlas reuse"
                        .into(),
                );
            }
        }
        let path = if enabled {
            Some(
                self.atlas_cache_path()
                    .ok_or("Save this rig's configuration before enabling atlas reuse")?,
            )
        } else {
            None
        };
        if let Some(worker) = &self.worker {
            worker.send(Command::AtlasCache(path))?;
        }
        self.reuse_atlas = enabled;
        Ok(())
    }
    pub fn use_fallback(&mut self, reason: &str) {
        if self
            .document()
            .is_ok_and(|doc| doc.camera_binding().ok().flatten().is_some())
        {
            self.startup_fallback = None;
            self.error = reason.into();
            return;
        }
        if let Some(doc) = self.startup_fallback.take() {
            self.sample_fallback_active = true;
            self.history = DocumentHistory::new(doc.text().to_owned());
            self.source_edit = doc.get("src_fn").unwrap_or("").to_owned();
            self.temporary_config = true;
            self.saved = self.history.text.clone();
            self.fallback_notice = format!("Camera unavailable ({reason}); playing sample video");
            let result = self.start();
            self.report(result);
        }
    }
    pub fn from_config_path(id: u64, path: PathBuf) -> Result<Self, String> {
        let text = crate::array::read_text(&path);
        let parsed = text.as_ref().map_err(Clone::clone).and_then(|text| {
            ConfigDocument::parse(text.clone()).map_err(|e| format!("{}: {e}", path.display()))
        });
        match parsed {
            Ok(document) => Ok(Self::new(id, document, Some(path))),
            Err(error) => {
                let mut failed = Self::new(
                    id,
                    ConfigDocument::parse("src_fn:\n".into()).map_err(|e| e.to_string())?,
                    Some(path),
                );
                let original = text.unwrap_or_else(|_| "Configuration unavailable\nLoad or repair the saved rig file before retrying.\n".into());
                failed.history = DocumentHistory::new(original.clone());
                failed.saved = original;
                failed.error = format!("Rig did not start: {error}");
                Ok(failed)
            }
        }
    }
    pub fn launch_state(&self) -> &'static str {
        if self.worker.is_none() || (self.snapshot.ended && self.snapshot.frames == 0) {
            "Did not start"
        } else if self.snapshot.ended {
            "Stopped"
        } else if self.snapshot.frames == 0 {
            "Opening"
        } else if self.tool != Tool::Inspect {
            "Calibrating"
        } else if self.paused {
            "Paused"
        } else {
            "Running"
        }
    }
    pub fn camera_format(
        &mut self,
        request: hejjetrac_camera::CameraFormatRequest,
    ) -> Result<(), String> {
        request.validate().map_err(|e| e.to_string())?;
        if self.source_edit.parse::<u32>().is_err() {
            return Err("Camera format requires a live camera source".into());
        }
        if let Some(caps) = &self.snapshot.camera {
            let supported = caps.formats.iter().any(|f| {
                f.width == request.width
                    && f.height == request.height
                    && request.fps.is_none_or(|fps| {
                        f.frame_rates
                            .iter()
                            .any(|r| fps >= r.min_fps - 1e-6 && fps <= r.max_fps + 1e-6)
                    })
            });
            if !supported {
                return Err("Requested dimensions/rate are not advertised by this camera; refresh camera controls".into());
            }
        }
        let mut doc = self.document()?;
        if doc.get("rust_calibration_size").is_some() {
            doc = hejjetrac_io::rescale_calibration_reference(&doc, request.width, request.height)
                .map_err(|e| e.to_string())?;
        }
        doc.set("rust_camera_width", &request.width.to_string())
            .map_err(|e| e.to_string())?;
        doc.set("rust_camera_height", &request.height.to_string())
            .map_err(|e| e.to_string())?;
        doc.set("src_fps", &request.fps.unwrap_or(-1.0).to_string())
            .map_err(|e| e.to_string())?;
        self.edit(doc.text().to_owned());
        self.start()
    }
    pub fn refresh_cameras(&mut self) {
        match hejjetrac_camera::list_cameras() {
            Ok(cameras) => {
                self.cameras = cameras;
                self.camera_error.clear();
            }
            Err(error) => self.camera_error = error.to_string(),
        }
    }
    pub fn set_display_fps(&mut self, fps: u32) -> Result<(), String> {
        if !(1..=60).contains(&fps) {
            return Err("Display FPS must be an integer between 1 and 60".into());
        }
        if let Some(worker) = &self.worker {
            worker.send(Command::DisplayFps(fps))?;
        }
        self.display_fps = fps;
        Ok(())
    }
    pub fn repaint_interval(&self) -> std::time::Duration {
        if self.snapshot.paused || self.snapshot.ended || self.worker.is_none() {
            std::time::Duration::from_millis(250)
        } else {
            std::time::Duration::from_secs_f64(1.0 / f64::from(self.display_fps))
        }
    }
    /// Daily calibration inspection changes only transient preview state.
    pub(crate) fn set_calibration_panel(&mut self, show: bool) {
        if show {
            if !self.show_calibration {
                self.calibration_previous_view = Some((self.full_image, self.show_overlays));
            }
            self.show_calibration = true;
            self.show_settings = true;
            self.show_config_text = false;
            if self.tool == Tool::Inspect {
                self.full_image = true;
                self.show_overlays = true;
            }
        } else {
            if self.show_calibration {
                self.show_settings = false;
            }
            self.show_calibration = false;
            if self.tool == Tool::Inspect
                && let Some((full_image, overlays)) = self.calibration_previous_view.take()
            {
                self.full_image = full_image;
                self.show_overlays = overlays;
            }
        }
    }
    pub(crate) fn set_settings_panel(&mut self, show: bool) {
        if self.show_calibration {
            self.set_calibration_panel(false);
        }
        self.show_settings = show;
    }
    pub(crate) fn restore_calibration_view_if_closed(&mut self) {
        if !self.show_calibration && self.tool == Tool::Inspect {
            self.set_calibration_panel(false);
        }
    }
    pub fn begin_recording(&mut self) {
        let result = self.queue_recording();
        self.report(result);
    }
    pub(crate) fn queue_recording(&mut self) -> Result<(), String> {
        let options = hejjetrac_control::RecordOptions {
            raw: self.record_raw,
            debug: self.record_debug,
        }
        .with_video();
        self.record_raw = options.raw;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = PathBuf::from(&self.recording_path).join(format!("take-{stamp}"));
        self.worker
            .as_ref()
            .ok_or("Start this rig before recording")?
            .send(Command::RecordOptions(
                path,
                options,
                hejjetrac_control::MediaRecordingTuning {
                    encoding: self.recording_encoding,
                    ..hejjetrac_control::MediaRecordingTuning::default()
                },
            ))
    }
    /// Stable identity order for compact titles, tiles and the manager.
    pub fn compact_identity(&self) -> String {
        self.window_title_for_width(0.0)
    }
    fn camera_identity(&self, input: &str) -> Option<(String, String)> {
        input.parse::<u32>().ok()?;
        let doc = self.document().ok();
        let binding = if self.worker.is_some() {
            self.running_camera_binding.as_deref()
        } else {
            doc.as_ref()
                .and_then(|d| d.get("rust_camera_id"))
                .filter(|s| !s.is_empty())
        };
        let actual = self
            .snapshot
            .actual_camera_id
            .or_else(|| input.parse().ok())
            .and_then(|id| self.cameras.iter().find(|camera| camera.id == id));
        let hardware = actual
            .and_then(|camera| camera.hardware_id.as_deref())
            .or(binding)
            .map(|id| {
                if id.starts_with("usb:") {
                    id.split(':').nth(3).unwrap_or(id)
                } else {
                    id
                }
            })
            .unwrap_or("serial unknown");
        let camera = self
            .snapshot
            .actual_camera_id
            .map(|v| v.to_string())
            .unwrap_or_else(|| input.to_owned());
        let label = &self.snapshot.source_label;
        let device = label
            .find("/dev/")
            .map(|start| label[start..].split([')', ' ']).next().unwrap_or("unknown"))
            .unwrap_or("device pending");
        let link = if binding.is_some() {
            "bound"
        } else {
            "unbound"
        };
        Some((
            format!("Cam {hardware} · {link} (#{camera})"),
            device.into(),
        ))
    }
    pub fn hardware_port(&self) -> &str {
        self.snapshot
            .actual_camera_id
            .and_then(|id| self.cameras.iter().find(|camera| camera.id == id))
            .and_then(|camera| camera.port_id.as_deref())
            .unwrap_or("Hardware port unavailable")
    }
    pub fn window_title(&self) -> String {
        self.window_title_for_width(self.window_rect.map(|r| r[2]).unwrap_or(520.0))
    }
    fn window_title_for_width(&self, width: f32) -> String {
        let source = if self.snapshot.source_label.is_empty() {
            if self.running_source.is_empty() {
                self.source_edit.clone()
            } else {
                self.running_source.clone()
            }
        } else {
            self.snapshot.source_label.clone()
        };
        let input = if self.running_source.is_empty() {
            &self.source_edit
        } else {
            &self.running_source
        };
        let short_source = short_source_label(input, &source);
        // Show applied transport state. A rejected draft edit stays visible in
        // the inspector, but must not relabel the old running output ports.
        let (udp, zmq) = if let Some(outputs) = &self.snapshot.outputs {
            let udp = outputs
                .udp_destination
                .as_deref()
                .filter(|_| outputs.udp_enabled)
                .and_then(|destination| destination.parse::<std::net::SocketAddr>().ok())
                .map(|destination| destination.port().to_string())
                .unwrap_or_else(|| "off".into());
            let zmq = outputs
                .zmq
                .as_ref()
                .map(|publisher| {
                    let port = publisher.endpoint.rsplit(':').next().unwrap_or("?");
                    if publisher.worker_alive && publisher.failures == 0 {
                        port.to_owned()
                    } else {
                        format!("{port} failed")
                    }
                })
                .unwrap_or_else(|| {
                    if self
                        .snapshot
                        .command_error
                        .as_deref()
                        .is_some_and(|error| error.starts_with("Live outputs are disabled"))
                    {
                        self.document()
                            .ok()
                            .and_then(|doc| doc.get("zmq_port").map(str::to_owned))
                            .filter(|port| port.parse::<u16>().is_ok_and(|p| p > 0))
                            .map(|port| format!("{port} failed"))
                            .unwrap_or_else(|| "off".into())
                    } else {
                        "off".into()
                    }
                });
            (udp, zmq)
        } else {
            self.document()
                .map(|doc| {
                    let port = |key| {
                        let value = doc.get(key).unwrap_or(if key == "zmq_port" {
                            hejjetrac_io::DEFAULT_ZMQ_PORT
                        } else {
                            "off"
                        });
                        if matches!(value, "-1" | "0") {
                            "off"
                        } else {
                            value
                        }
                    };
                    (port("sock_port").to_owned(), port("zmq_port").to_owned())
                })
                .unwrap_or_default()
        };
        if let Some((camera, device)) = self.camera_identity(input) {
            let suffix = if width >= 650.0 || (zmq == "off" && udp != "off") {
                format!(" · UDP {udp}")
            } else {
                String::new()
            };
            return format!(
                "{} · {camera} · ZMQ {zmq} · {device}{suffix}",
                self.display_name()
            );
        }
        if width >= 900.0 {
            format!(
                "{} · ZMQ {zmq} · {short_source} · UDP {udp} · HejjéTrac · {source}",
                self.display_name()
            )
        } else if width >= 650.0 {
            format!(
                "{} · ZMQ {zmq} · {short_source} · UDP {udp} · HejjéTrac",
                self.display_name()
            )
        } else {
            let output = if zmq != "off" {
                format!("ZMQ {zmq}")
            } else if udp != "off" {
                format!("UDP {udp}")
            } else {
                "Outputs off".into()
            };
            format!("{} · {output} · {short_source}", self.display_name())
        }
    }
    pub fn display_name(&self) -> String {
        self.document()
            .ok()
            .and_then(|d| {
                d.get("rust_instance_name")
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| s.chars().take(64).collect())
            })
            .or_else(|| {
                let path = self.path.as_ref().or(self.source_base.as_ref())?;
                // Original FicTrac rigs usually keep a plain config.txt inside
                // VR3/ (or BR3/), without a Rust-specific instance name.
                [path.file_stem(), path.parent().and_then(|p| p.file_name())]
                    .into_iter()
                    .flatten()
                    .filter_map(|name| name.to_str())
                    .find(|name| {
                        name.strip_prefix("VR")
                            .or_else(|| name.strip_prefix("BR"))
                            .is_some_and(|suffix| {
                                !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit())
                            })
                    })
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| format!("VR{}", self.id))
    }
    pub fn document(&self) -> Result<ConfigDocument, String> {
        ConfigDocument::parse(self.history.text.clone()).map_err(|e| e.to_string())
    }
    pub fn edit(&mut self, text: String) {
        self.history.replace(text);
        self.changed = Some(Instant::now());
    }
    pub fn fields(&mut self, values: &[(&str, String)]) -> Result<(), String> {
        let mut doc = self.document()?;
        for (key, value) in values {
            doc.set(key, value).map_err(|e| e.to_string())?;
        }
        if doc.get("rust_calibration_size").is_some()
            && values.iter().any(|(key, _)| {
                matches!(
                    *key,
                    "roi_circ" | "roi_ignr" | "c2a_cnrs_xy" | "c2a_cnrs_yz" | "c2a_cnrs_xz"
                )
            })
        {
            let [width, height]: [usize; 2] = doc
                .vector("rust_calibration_size")
                .map_err(|e| e.to_string())?;
            doc = hejjetrac_io::capture_calibration_reference(&doc, width, height)
                .map_err(|e| e.to_string())?;
        }
        self.edit(doc.text().to_owned());
        self.source_edit = doc.get("src_fn").unwrap_or("").to_owned();
        Ok(())
    }
    pub fn start(&mut self) -> Result<(), String> {
        let doc = self.document()?;
        self.ensure_source_available(&doc)?;
        let source = doc
            .source(self.path.as_deref().or(self.source_base.as_deref()))
            .map_err(|e| e.to_string())?;
        if matches!(source, SourceSpec::Camera(_)) {
            hejjetrac_camera::request_permission().map_err(|e| e.to_string())?;
        }
        self.worker = None;
        self.camera_edit = None;
        self.snapshot = Snapshot::default();
        self.revision = 0;
        self.textures.clear();
        self.texture_image = None;
        self.texture_frame = u64::MAX;
        self.running_source = doc.get("src_fn").unwrap_or("").into();
        self.running_camera_binding = doc
            .camera_binding()
            .map_err(|e| e.to_string())?
            .map(str::to_owned);
        self.running_fps = doc.number("src_fps", -1.0);
        self.worker = Some(Worker::start_with_cache(
            source,
            doc,
            self.realtime,
            self.path.clone().or(self.source_base.clone()),
            self.reuse_atlas.then(|| self.atlas_cache_path()).flatten(),
        )?);
        if let Some(worker) = &self.worker {
            worker.send(Command::DisplayFps(self.display_fps))?;
        }
        self.paused = false;
        self.changed = None;
        self.error.clear();
        Ok(())
    }
    pub fn ensure_source_available(&self, doc: &ConfigDocument) -> Result<(), String> {
        let actual_camera = self
            .worker
            .as_ref()
            .map(|w| w.latest().actual_camera_id)
            .unwrap_or(self.snapshot.actual_camera_id);
        let id = doc
            .get("src_fn")
            .and_then(|source| source.parse::<u32>().ok());
        let id = match (id, doc.camera_binding().map_err(|e| e.to_string())?) {
            (Some(_), Some(binding)) => Some(
                hejjetrac_camera::resolve_camera(
                    &hejjetrac_camera::list_cameras().map_err(|e| e.to_string())?,
                    binding,
                )
                .map_err(|e| e.to_string())?,
            ),
            (id, _) => id,
        };
        if let Some(id) = id
            && let Some(owner) = self.occupied_cameras.get(&id)
        {
            return Err(format!(
                "Camera {id} is already used by {owner}. Close that rig or choose a free camera."
            ));
        }
        if let Some(id) = id
            && self.worker.is_some()
            && actual_camera.or_else(|| self.running_source.parse::<u32>().ok()) != Some(id)
            && !hejjetrac_camera::list_cameras()
                .map_err(|e| e.to_string())?
                .iter()
                .any(|camera| camera.id == id)
        {
            return Err(format!(
                "Camera {id} is unavailable. Refresh the camera list and choose a connected device; the active source was preserved."
            ));
        }
        Ok(())
    }
    pub fn select_camera(&mut self, id: u32) -> Result<(), String> {
        if let Some(worker) = &self.worker {
            self.snapshot = worker.latest();
        }
        let mut doc = self.document()?;
        if let Some(binding) = doc.camera_binding().map_err(|e| e.to_string())? {
            let selected = self.cameras.iter().find(|c| c.id == id);
            if !selected.is_some_and(|camera| {
                camera.hardware_id.as_deref() == Some(binding)
                    || camera.port_id.as_deref() == Some(binding)
            }) {
                return Err("This rig is bound to another camera. Unlink it before selecting a replacement.".into());
            }
            if self.running_camera_binding.as_deref() == Some(binding)
                && self.worker.is_some()
                && !self.snapshot.ended
            {
                return Ok(());
            }
        }
        doc.set("src_fn", &id.to_string())
            .map_err(|e| e.to_string())?;
        self.ensure_source_available(&doc)?;
        if self
            .snapshot
            .actual_camera_id
            .or_else(|| self.running_source.parse::<u32>().ok())
            == Some(id)
            && self.worker.is_some()
            && !self.snapshot.ended
        {
            if doc.text() != self.history.text {
                // Selecting the running camera can also repair a rejected
                // source draft without needlessly closing that capture stream.
                self.edit(doc.text().to_owned());
                self.source_edit = id.to_string();
                self.apply()?;
            }
            return Ok(());
        }
        self.edit(doc.text().to_owned());
        self.source_edit = id.to_string();
        self.start()
    }
    pub fn bind_camera(&mut self, hardware_id: Option<&str>) -> Result<(), String> {
        if let Some(worker) = &self.worker {
            self.snapshot = worker.latest();
        }
        let mut doc = self.document()?;
        if let Some(value) = hardware_id {
            hejjetrac_camera::validate_hardware_id(value).map_err(|e| e.to_string())?;
            if !doc.get("src_fn").is_some_and(|v| v.parse::<u32>().is_ok()) {
                return Err("Select a camera before linking this rig to hardware".into());
            }
        } else if doc.camera_binding().map_err(|e| e.to_string())?.is_some()
            && let Some(id) = self.snapshot.actual_camera_id
        {
            // The saved ordinal may predate USB renumbering. Unlink the stream
            // the user is actually viewing, never jump to that stale ordinal.
            doc.set("src_fn", &id.to_string())
                .map_err(|e| e.to_string())?;
        }
        doc.set("rust_camera_id", hardware_id.unwrap_or(""))
            .map_err(|e| e.to_string())?;
        self.ensure_source_available(&doc)?;
        if let Some(image) = &self.snapshot.image {
            hejjetrac_io::load_config_text(doc.text(), image.width, image.height)
                .map_err(|e| e.to_string())?;
        }
        let history = self.history.clone();
        let changed = self.changed;
        self.edit(doc.text().to_owned());
        if let Err(error) = self.apply() {
            self.history = history;
            self.changed = changed;
            return Err(error);
        }
        self.camera_binding_draft = None;
        Ok(())
    }
    pub fn apply(&mut self) -> Result<(), String> {
        if let Some(worker) = &self.worker {
            self.snapshot = worker.latest();
        }
        let mut doc = self.document()?;
        self.keep_unlinked_camera(&mut doc)?;
        self.ensure_source_available(&doc)?;
        if let Some(image) = &self.snapshot.image {
            hejjetrac_io::load_config_text(doc.text(), image.width, image.height)
                .map_err(|e| format!("Configuration was not applied: {e}"))?;
        }
        let source = doc.get("src_fn").unwrap_or("");
        let binding = doc.camera_binding().map_err(|e| e.to_string())?;
        let assignment_changed =
            source != self.running_source || binding != self.running_camera_binding.as_deref();
        if self.worker.is_some() {
            if source.parse::<u32>().is_ok() && doc.number("src_fps", -1.0) != self.running_fps {
                return self.start();
            }
            if assignment_changed {
                let next_camera = match (source.parse::<u32>().ok(), binding) {
                    (Some(_), Some(binding)) => Some(
                        hejjetrac_camera::resolve_camera(
                            &hejjetrac_camera::list_cameras().map_err(|e| e.to_string())?,
                            binding,
                        )
                        .map_err(|e| e.to_string())?,
                    ),
                    (id, _) => id,
                };
                if next_camera.is_none() || next_camera != self.snapshot.actual_camera_id {
                    return self.start();
                }
            }
        }
        let next_source = source.to_owned();
        let next_binding = binding.map(str::to_owned);
        let revision = self
            .revision
            .checked_add(1)
            .ok_or("Configuration revision counter overflow")?;
        if let Some(worker) = &self.worker {
            worker.send(Command::Configure(doc.clone(), revision))?;
        }
        self.revision = revision;
        if doc.text() != self.history.text {
            self.edit(doc.text().to_owned());
        }
        self.running_source = next_source;
        self.running_camera_binding = next_binding;
        self.source_edit = self.running_source.clone();
        self.changed = None;
        Ok(())
    }
    pub(crate) fn keep_unlinked_camera(&self, doc: &mut ConfigDocument) -> Result<(), String> {
        let actual = self
            .worker
            .as_ref()
            .map(|w| w.latest().actual_camera_id)
            .unwrap_or(self.snapshot.actual_camera_id);
        if self.running_camera_binding.is_some()
            && doc.camera_binding().map_err(|e| e.to_string())?.is_none()
            && doc.get("src_fn") == Some(self.running_source.as_str())
            && let Some(id) = actual
        {
            doc.set("src_fn", &id.to_string())
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    pub fn load(&mut self, path: PathBuf) -> Result<(), String> {
        let text = crate::array::read_text(&path)?;
        let doc = ConfigDocument::parse(text).map_err(|e| e.to_string())?;
        self.save_view()?;
        self.worker = None;
        self.sample_fallback_active = false;
        self.record_raw = hejjetrac_control::RecordOptions::from_document(&doc)
            .with_video()
            .raw;
        self.record_debug = doc.boolean("save_debug");
        self.camera_binding_draft = None;
        self.recording_encoding = hejjetrac_control::RecordingEncoding::Configured;
        self.history = DocumentHistory::new(doc.text().to_owned());
        self.zmq_resume_port = None;
        self.udp_resume_port = None;
        self.saved = doc.text().to_owned();
        self.path = Some(path);
        self.disk_conflict = false;
        self.source_edit = doc.get("src_fn").unwrap_or("").into();
        self.points.clear();
        self.square_slots = None;
        self.pending_drag = None;
        self.tool = Tool::Inspect;
        self.calibration_review = true;
        self.calibration_redraw = false;
        self.edit_existing = false;
        self.paused = self
            .calibration_previous_paused
            .take()
            .unwrap_or(self.paused);
        self.set_calibration_panel(false);
        self.restore_view();
        self.start()
    }
    pub fn video(&mut self, path: PathBuf) -> Result<(), String> {
        let path = fs::canonicalize(path).map_err(|e| e.to_string())?;
        self.fields(&[("src_fn", path.to_string_lossy().into_owned())])?;
        self.start()
    }
    pub fn save(&mut self, as_new: bool) -> Result<(), String> {
        let mut doc = self.document()?;
        if let Some(image) = &self.snapshot.image {
            hejjetrac_io::load_config_text(doc.text(), image.width, image.height)
                .map_err(|e| format!("Configuration was not saved: {e}"))?;
        }
        let path = if as_new || self.path.is_none() || self.temporary_config {
            rfd::FileDialog::new()
                .add_filter("Tracking configuration", &["txt"])
                .set_file_name("config.txt")
                .save_file()
                .ok_or("Save cancelled")?
        } else {
            self.path.clone().ok_or("No config path")?
        };
        if self.path.as_ref() != Some(&path)
            && let Ok(hejjetrac_io::SourceSpec::Video(source)) =
                doc.source(self.path.as_deref().or(self.source_base.as_deref()))
            && let Ok(absolute) = fs::canonicalize(source)
        {
            // Keep Save As attached to the same video after moving the config directory.
            doc.set("src_fn", &absolute.to_string_lossy())
                .map_err(|e| e.to_string())?;
        }
        if !as_new
            && path.is_file()
            && fs::read_to_string(&path).map_err(|e| e.to_string())? != self.saved
        {
            return Err(
                "File changed on disk. Reload or Save As; the external edit was preserved.".into(),
            );
        }
        let tmp = path.with_extension(format!("hejjetrac-{}.tmp", std::process::id()));
        fs::write(&tmp, doc.text())
            .and_then(|_| fs::rename(&tmp, &path))
            .map_err(|e| {
                format!(
                    "Cannot save {}: {e}. Use Save As to choose a writable configuration folder.",
                    path.display()
                )
            })?;
        self.history.replace(doc.text().to_owned());
        self.source_edit = doc.get("src_fn").unwrap_or("").into();
        self.saved = doc.text().to_owned();
        self.path = Some(path.clone());
        self.source_base = Some(path);
        self.disk_conflict = false;
        self.temporary_config = false;
        self.save_view()?;
        self.status = "Configuration saved".into();
        Ok(())
    }
    fn observe_worker_snapshot(&mut self, snapshot: Snapshot) {
        // A new applied revision acknowledges recovery from an earlier rejected
        // configuration. Clear only that exact error, preserving unrelated local
        // errors and any current command failure.
        let recovered_configuration = snapshot.command_error.is_none()
            && snapshot.revision > self.snapshot.revision
            && self
                .snapshot
                .command_error
                .as_ref()
                .is_some_and(|previous| self.error == *previous);
        if let Some(error) = &snapshot.command_error {
            self.error = error.clone();
        } else if recovered_configuration {
            self.error.clear();
        }
        self.snapshot = snapshot;
    }
    pub fn update(&mut self) {
        if let Some(worker) = &self.worker {
            let snapshot = worker.latest();
            self.observe_worker_snapshot(snapshot);
        }
        if self.snapshot.ended && self.snapshot.frames == 0 && self.startup_fallback.is_some() {
            let reason = self.snapshot.notice.clone();
            self.use_fallback(&reason);
        } else if self.snapshot.frames > 0 {
            self.startup_fallback = None;
        }
        if self.auto_apply && self.changed.is_some_and(|t| t.elapsed().as_millis() > 400) {
            // A rejected draft is retried only after another edit or Apply,
            // rather than revalidating it on every repaint indefinitely.
            self.changed = None;
            let r = self.apply();
            self.report(r);
        }
        if !self.temporary_config && self.last_disk_check.elapsed().as_millis() > 600 {
            self.last_disk_check = Instant::now();
            if let Some(path) = &self.path
                && let Ok(text) = fs::read_to_string(path)
                && text != self.saved
            {
                if self.history.text == self.saved {
                    match ConfigDocument::parse(text.clone()) {
                        Ok(doc) => {
                            self.history.replace(text.clone());
                            self.saved = text;
                            self.source_edit = doc.get("src_fn").unwrap_or("").into();
                            self.changed = Some(Instant::now());
                            self.status = "Reloaded an external file edit".into();
                        }
                        Err(e) => {
                            self.error = format!("External config is invalid: {e}");
                        }
                    }
                } else {
                    self.disk_conflict = true;
                }
            }
        }
    }
    pub fn report(&mut self, result: Result<(), String>) {
        if let Err(e) = result {
            self.error = e;
        } else {
            self.error.clear();
        }
    }
    pub fn send(&mut self, command: Command) {
        if let Some(worker) = &self.worker {
            let r = worker.send(command);
            self.report(r);
        }
    }
}

fn short_source_label(input: &str, actual: &str) -> String {
    if let Ok(id) = input.parse::<u32>() {
        // Use an actual V4L path when the backend provided one; never infer a
        // device path from an enumeration ordinal on a different platform.
        if let Some(start) = actual.find("/dev/video") {
            let path = &actual[start..];
            let end = path
                .find(|c: char| c.is_whitespace() || c == ')')
                .unwrap_or(path.len());
            return path[..end].to_owned();
        }
        return format!("Camera {id}");
    }
    actual
        .rsplit(['/', '\\'])
        .find(|s| !s.is_empty())
        .unwrap_or(actual)
        .to_owned()
}

pub(crate) struct CalibrationCache {
    pub text: String,
    pub width: usize,
    pub height: usize,
    pub config: Result<hejjetrac_core::Config, String>,
    pub crop: Option<egui::Rect>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unlink_queue_failure_preserves_config_revision_and_camera_draft() -> Result<(), String> {
        let binding = "usb:0c45:6366:M01:video:0";
        let text =
            format!("src_fn:0\nrust_camera_id:{binding}\nvfov:45\nroi_c:{{0,0,1}}\nroi_r:0.2\n");
        let mut s = Session::new(
            1,
            ConfigDocument::parse(text.clone()).map_err(|e| e.to_string())?,
            None,
        );
        s.running_source = "0".into();
        s.running_camera_binding = Some(binding.into());
        s.camera_binding_draft = Some("retained draft".into());
        s.worker = Some(Worker::disconnected_for_test(Snapshot {
            actual_camera_id: Some(6),
            ..Snapshot::default()
        }));
        assert!(s.bind_camera(None).is_err());
        assert_eq!(s.history.text, text);
        assert_eq!(s.revision, 0);
        assert_eq!(s.camera_binding_draft.as_deref(), Some("retained draft"));
        assert_eq!(s.running_source, "0");
        let mut unlinked = s.document()?;
        unlinked
            .set("rust_camera_id", "")
            .map_err(|e| e.to_string())?;
        s.occupied_cameras
            .insert(0, "different rig using the old ordinal".into());
        s.keep_unlinked_camera(&mut unlinked)?;
        assert_eq!(unlinked.get("src_fn"), Some("6"));
        s.ensure_source_available(&unlinked)?;
        Ok(())
    }
    #[test]
    fn unlink_keeps_the_open_camera_after_device_numbers_change() -> Result<(), String> {
        let doc = ConfigDocument::parse("src_fn:0\nrust_camera_id:usb:0c45:6366:M01:video:0\nsock_port:1111\nzmq_port:9871\nroi_r:0.11\n".into()).map_err(|e| e.to_string())?;
        let mut session = Session::new(1, doc, None);
        session.snapshot.actual_camera_id = Some(6);
        session.bind_camera(None)?;
        let unlinked = session.document()?;
        assert_eq!(unlinked.get("src_fn"), Some("6"));
        assert_eq!(unlinked.camera_binding().map_err(|e| e.to_string())?, None);
        assert_eq!(unlinked.get("sock_port"), Some("1111"));
        assert_eq!(unlinked.get("zmq_port"), Some("9871"));
        assert_eq!(unlinked.get("roi_r"), Some("0.11"));
        Ok(())
    }
    #[test]
    fn a_bound_missing_camera_never_uses_startup_sample_and_invalid_binding_is_atomic()
    -> Result<(), String> {
        let text = "src_fn:99999\nrust_camera_id:usb:ffff:ffff:missing:video:0\nsock_port:1114\nzmq_port:9874\n";
        let document = ConfigDocument::parse(text.into()).map_err(|e| e.to_string())?;
        let mut session = Session::new(4, document, None);
        session.startup_fallback =
            Some(ConfigDocument::parse("src_fn:sample.mp4\n".into()).map_err(|e| e.to_string())?);
        session.use_fallback("required camera missing");
        assert!(session.worker.is_none());
        assert!(session.startup_fallback.is_none());
        assert_eq!(session.history.text, text);
        assert_eq!(session.error, "required camera missing");
        assert!(session.bind_camera(Some("bad#id")).is_err());
        assert_eq!(session.history.text, text);
        Ok(())
    }
    #[test]
    fn successful_configuration_ack_clears_only_the_superseded_command_error()
    -> Result<(), Box<dyn std::error::Error>> {
        let document = ConfigDocument::parse("src_fn:sample.mp4\n".into())?;
        let mut session = Session::new(1, document, None);
        let rejected = Snapshot {
            revision: 2,
            command_error: Some("Configuration rejected: occupied publisher port".into()),
            ..Snapshot::default()
        };
        session.observe_worker_snapshot(rejected.clone());
        assert_eq!(
            session.error,
            rejected.command_error.as_deref().unwrap_or("")
        );
        let applied = Snapshot {
            revision: 4,
            notice: "Configuration applied; tracking epoch reset".into(),
            ..Snapshot::default()
        };
        session.observe_worker_snapshot(applied.clone());
        assert!(session.error.is_empty());

        session.observe_worker_snapshot(rejected.clone());
        session.error = "Cannot save configuration: folder is read-only".into();
        session.observe_worker_snapshot(applied.clone());
        assert_eq!(
            session.error,
            "Cannot save configuration: folder is read-only"
        );

        session.observe_worker_snapshot(rejected.clone());
        session.observe_worker_snapshot(Snapshot {
            revision: 2,
            ..Snapshot::default()
        });
        assert!(
            !session.error.is_empty(),
            "No successful revision was acknowledged"
        );
        session.observe_worker_snapshot(rejected);
        session.observe_worker_snapshot(Snapshot {
            revision: 4,
            command_error: Some("Configuration rejected: publisher unavailable".into()),
            ..Snapshot::default()
        });
        assert_eq!(
            session.error,
            "Configuration rejected: publisher unavailable"
        );
        Ok(())
    }

    #[test]
    fn real_publisher_retry_clears_the_rejected_port_banner_after_apply()
    -> Result<(), Box<dyn std::error::Error>> {
        fn wait_for(
            session: &mut Session,
            predicate: impl Fn(&Session) -> bool,
        ) -> Result<(), String> {
            let deadline = Instant::now() + std::time::Duration::from_secs(15);
            loop {
                session.update();
                if predicate(session) {
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    return Err(format!(
                        "Session response timed out: revision={}, requested={}, command_error={:?}, error={}, notice={}",
                        session.snapshot.revision,
                        session.revision,
                        session.snapshot.command_error,
                        session.error,
                        session.snapshot.notice
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut document = ConfigDocument::parse(fs::read_to_string(
            root.join("examples/upstream-config.txt"),
        )?)?;
        document.set(
            "src_fn",
            &root.join("examples/sample.mp4").to_string_lossy(),
        )?;
        document.set("zmq_port", "-1")?;
        document.set("sock_port", "-1")?;
        document.set("save_raw", "n")?;
        document.set("save_debug", "n")?;
        let mut session = Session::new(1, document, None);
        session.auto_apply = false;
        session.start()?;
        wait_for(&mut session, |s| s.snapshot.frames >= 3)?;
        session.send(Command::Pause(true));
        wait_for(&mut session, |s| s.snapshot.paused)?;
        let source_frames = session.snapshot.frames;
        let busy = std::net::TcpListener::bind("0.0.0.0:0")?;
        session.fields(&[("zmq_port", busy.local_addr()?.port().to_string())])?;
        session.apply()?;
        wait_for(&mut session, |s| s.snapshot.command_error.is_some())?;
        assert!(session.error.contains("Configuration rejected"));
        assert!(session.error.contains("rollback succeeded"));
        assert_eq!(session.snapshot.frames, source_frames);

        let free = std::net::TcpListener::bind("0.0.0.0:0")?;
        let free_port = free.local_addr()?.port();
        drop(free);
        session.fields(&[("zmq_port", free_port.to_string())])?;
        session.apply()?;
        assert!(
            !session.error.is_empty(),
            "A queued request is not an acknowledgement"
        );
        wait_for(&mut session, |s| {
            s.snapshot.revision == s.revision && s.snapshot.command_error.is_none()
        })?;
        assert!(
            session.error.is_empty(),
            "The rejected-port banner survived a successful apply"
        );
        assert_eq!(session.snapshot.frames, source_frames);
        let publisher = session
            .snapshot
            .outputs
            .as_ref()
            .and_then(|outputs| outputs.zmq.as_ref())
            .ok_or("Publisher status missing after successful apply")?;
        assert_eq!(publisher.endpoint, format!("tcp://0.0.0.0:{free_port}"));
        assert!(publisher.worker_alive);
        Ok(())
    }
    #[test]
    fn title_uses_applied_ports_and_source_instead_of_rejected_draft()
    -> Result<(), Box<dyn std::error::Error>> {
        let document = ConfigDocument::parse(
            "rust_instance_name:VR7\nsrc_fn:/draft/input.mp4\nsock_port:2222\nzmq_port:9999\n"
                .into(),
        )?;
        let mut session = Session::new(7, document, None);
        session.snapshot.source_label = "/actual/input.mp4".into();
        session.snapshot.outputs = Some(hejjetrac_io::OutputStatus {
            udp_enabled: true,
            udp_destination: Some("127.0.0.1:1111".into()),
            serial_enabled: false,
            zmq_payload: None,
            zmq: Some(hejjetrac_io::ZmqOutputStatus {
                endpoint: "tcp://127.0.0.1:9871".into(),
                queue_capacity: 256,
                queue_len: 0,
                enqueued: 0,
                published: 0,
                dropped: 0,
                failures: 0,
                worker_alive: true,
                last_error: None,
            }),
        });
        let title = session.window_title();
        assert!(title.contains("VR7"));
        assert_eq!(title, "VR7 · ZMQ 9871 · input.mp4");
        assert!(
            session
                .window_title_for_width(960.0)
                .contains("/actual/input.mp4")
        );
        assert!(session.window_title_for_width(700.0).contains("UDP 1111"));
        assert!(!title.contains("2222") && !title.contains("9999"));
        if let Some(publisher) = session
            .snapshot
            .outputs
            .as_mut()
            .and_then(|outputs| outputs.zmq.as_mut())
        {
            publisher.worker_alive = false;
            publisher.failures = 1;
        }
        assert!(session.window_title().contains("ZMQ 9871 failed"));
        session.snapshot.outputs = None;
        assert!(session.window_title().contains("ZMQ 9999"));
        assert!(session.window_title_for_width(700.0).contains("UDP 2222"));
        Ok(())
    }
    #[test]
    fn camera_titles_are_short_and_never_invent_device_paths() {
        assert_eq!(
            short_source_label("2", "Huge camera name (AVFoundation: 0x00000001)"),
            "Camera 2"
        );
        assert_eq!(
            short_source_label("7", "External camera (/dev/video7)"),
            "/dev/video7"
        );
        assert_eq!(
            short_source_label("C:\\Videos\\trial.mp4", "C:\\Videos\\trial.mp4"),
            "trial.mp4"
        );
    }
    #[test]
    fn loading_another_config_refreshes_record_defaults_and_keeps_manual_edits_local()
    -> Result<(), String> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut doc = ConfigDocument::parse(
            fs::read_to_string(root.join("examples/upstream-config.txt"))
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        doc.set(
            "src_fn",
            &root.join("examples/sample.mp4").to_string_lossy(),
        )
        .map_err(|error| error.to_string())?;
        doc.set("save_raw", "y")
            .map_err(|error| error.to_string())?;
        doc.set("save_debug", "y")
            .map_err(|error| error.to_string())?;
        let mut session = Session::new(1, doc.clone(), None);
        assert!(session.record_raw && session.record_debug);
        for key in ["save_raw", "save_debug"] {
            doc.set(key, "n").map_err(|error| error.to_string())?;
        }
        doc.set("sock_port", "0")
            .map_err(|error| error.to_string())?;
        doc.set("zmq_port", "0")
            .map_err(|error| error.to_string())?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("hejjetrac-load-record-defaults-{stamp}"));
        fs::create_dir(&directory).map_err(|error| error.to_string())?;
        let path = directory.join("config.txt");
        fs::write(&path, doc.text()).map_err(|error| error.to_string())?;
        session.load(path)?;
        assert!(session.record_raw && !session.record_debug);
        let deadline = Instant::now() + std::time::Duration::from_secs(15);
        while session.snapshot.frames < 2 {
            session.update();
            if Instant::now() >= deadline {
                return Err("loaded real-video source did not advance".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        // Ordinary numerical edits preserve the user's transient recording
        // selection; opening a different config is the reset boundary.
        session.record_raw = true;
        session.fields(&[("opt_tol", "0.002".into())])?;
        session.apply()?;
        assert!(session.record_raw && !session.record_debug);
        assert_eq!(session.document()?.get("save_raw"), Some("n"));
        session.worker = None;
        fs::remove_dir_all(directory).map_err(|error| error.to_string())?;
        Ok(())
    }
    #[test]
    fn record_selection_starts_from_the_loaded_config_flags() -> Result<(), String> {
        for raw in [false, true] {
            for debug in [false, true] {
                let doc = ConfigDocument::parse(format!(
                    "src_fn:4\nsave_raw:{}\nsave_debug:{}\n",
                    if raw { "y" } else { "n" },
                    if debug { "y" } else { "n" },
                ))
                .map_err(|error| error.to_string())?;
                let session = Session::new(1, doc, None);
                assert_eq!(session.record_raw, raw || !debug);
                assert_eq!(session.record_debug, debug);
            }
        }
        Ok(())
    }
    #[test]
    fn repeated_camera_selection_and_busy_peer_keep_the_existing_worker() -> Result<(), String> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut doc = ConfigDocument::parse(
            fs::read_to_string(root.join("examples/upstream-config.txt"))
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        doc.set("src_fn", "4").map_err(|e| e.to_string())?;
        let mut session = Session::new(1, doc.clone(), None);
        // Exercise the real decoding/tracking worker while representing its
        // source ownership as a camera; this test never opens a hardware device.
        session.running_source = "4".into();
        session.worker = Some(Worker::start(
            SourceSpec::Video(root.join("examples/sample.mp4")),
            doc,
            true,
        )?);
        let deadline = Instant::now() + std::time::Duration::from_secs(15);
        while session.snapshot.frames < 3 {
            session.update();
            if Instant::now() >= deadline {
                return Err("real source worker did not deliver frames".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let shared = session
            .worker
            .as_ref()
            .ok_or("missing running worker")?
            .snapshot
            .clone();
        let original = session.history.text.clone();
        session.select_camera(4)?;
        assert_eq!(session.history.text, original);
        assert_eq!(session.revision, 0);
        session.occupied_cameras.insert(2, "VR2 (window 2)".into());
        assert!(session.select_camera(2).is_err());
        assert_eq!(session.history.text, original);
        assert!(session.snapshot.image.is_some());
        session.fields(&[("src_fn", "2".into())])?;
        assert!(session.start().is_err());
        session.select_camera(4)?;
        assert_eq!(session.document()?.get("src_fn"), Some("4"));
        assert_eq!(session.source_edit, "4");
        assert!(std::sync::Arc::ptr_eq(
            &shared,
            &session.worker.as_ref().ok_or("worker was lost")?.snapshot,
        ));
        Ok(())
    }
    #[test]
    fn compact_title_prioritizes_the_active_udp_output() -> Result<(), String> {
        let doc = ConfigDocument::parse("src_fn:4\nsock_port:1113\nzmq_port:-1\n".into())
            .map_err(|e| e.to_string())?;
        let mut session = Session::new(3, doc, None);
        session.cameras.clear();
        assert_eq!(
            session.window_title(),
            "VR3 · Cam serial unknown · unbound (#4) · ZMQ off · device pending · UDP 1113"
        );
        session.snapshot.outputs = Some(hejjetrac_io::OutputStatus {
            udp_enabled: true,
            udp_destination: Some("127.0.0.1:1114".into()),
            serial_enabled: false,
            zmq_payload: None,
            zmq: None,
        });
        assert_eq!(
            session.window_title(),
            "VR3 · Cam serial unknown · unbound (#4) · ZMQ off · device pending · UDP 1114"
        );
        if let Some(outputs) = &mut session.snapshot.outputs {
            outputs.udp_enabled = false;
        }
        assert_eq!(
            session.window_title(),
            "VR3 · Cam serial unknown · unbound (#4) · ZMQ off · device pending"
        );
        session.snapshot.actual_camera_id = Some(4);
        session.cameras.push(hejjetrac_camera::CameraInfo {
            id: 4,
            name: "Test camera".into(),
            hardware_id: Some("usb:0c45:6366:A03:video:0".into()),
            port_id: Some("usb-port:test-connector".into()),
        });
        assert!(session.compact_identity().contains("Cam A03 · unbound"));
        assert_eq!(session.hardware_port(), "usb-port:test-connector");
        session.fields(&[("zmq_port", "9873".into())])?;
        session.snapshot.command_error = Some("Live outputs are disabled: port occupied".into());
        assert!(session.compact_identity().contains("ZMQ 9873 failed"));
        Ok(())
    }
    #[test]
    fn legacy_rig_names_come_from_the_selected_config() -> Result<(), String> {
        for (path, expected) in [
            ("/rigs/VR3/config.txt", "VR3"),
            ("/rigs/BR4/config.txt", "BR4"),
            ("/rigs/VR2.txt", "VR2"),
            ("/rigs/VR/config.txt", "VR1"),
            ("/rigs/VR3draft/config.txt", "VR1"),
        ] {
            let doc = ConfigDocument::parse("src_fn:4\n".into()).map_err(|e| e.to_string())?;
            let mut session = Session::new(1, doc, Some(PathBuf::from(path)));
            assert_eq!(session.display_name(), expected);
            assert!(session.window_title().starts_with(expected));
            session.fields(&[("rust_instance_name", "Rig A".into())])?;
            assert_eq!(session.display_name(), "Rig A");
        }
        Ok(())
    }
}
