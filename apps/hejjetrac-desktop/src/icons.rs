//! Original Lucide artwork, rendered offline and tinted by the native UI.
//! Pinned SVG sources and ISC/Feather MIT notices live in assets/lucide.
use eframe::egui::{self, TextureHandle, Vec2};
#[derive(Clone, Copy)]
pub enum Icon {
    Reset,
    Record,
    Settings,
    Diagnostics,
    Refresh,
    Crop,
    More,
    Camera,
    Calibration,
    Play,
    Pause,
    Step,
    Link,
    Unlink,
    Grid,
    Rigs,
}
fn artwork(icon: Icon, active: bool) -> (&'static str, &'static [u8]) {
    macro_rules! asset {
        ($name:literal) => {
            (
                $name,
                include_bytes!(concat!("../../../assets/lucide/", $name, ".png")),
            )
        };
    }
    match icon {
        Icon::Reset => asset!("rotate-ccw"),
        Icon::Record if active => asset!("square"),
        Icon::Record => asset!("circle-dot"),
        Icon::Settings => asset!("sliders-horizontal"),
        Icon::Diagnostics | Icon::Grid | Icon::Rigs => asset!("layout-dashboard"),
        Icon::Refresh => asset!("refresh-cw"),
        Icon::Crop if active => asset!("maximize"),
        Icon::Crop => asset!("minimize"),
        Icon::More => asset!("ellipsis"),
        Icon::Camera => asset!("camera"),
        Icon::Calibration => asset!("circle-dot"),
        Icon::Play => asset!("play"),
        Icon::Pause => asset!("pause"),
        Icon::Step => asset!("skip-forward"),
        Icon::Link | Icon::Unlink => asset!("camera"),
    }
}
fn native_button(ui: &egui::Ui, icon: Icon, active: bool) -> egui::Button<'static> {
    if matches!(
        icon,
        Icon::Calibration | Icon::Link | Icon::Unlink | Icon::Grid | Icon::Rigs | Icon::Diagnostics
    ) {
        return egui::Button::new("")
            .min_size(Vec2::splat(30.0))
            .selected(active)
            .frame(active);
    }
    let (name, bytes) = artwork(icon, active);
    let key = egui::Id::new(("lucide", name));
    let texture = ui
        .ctx()
        .data_mut(|data| data.get_temp::<TextureHandle>(key));
    let texture = texture.or_else(|| {
        let decoded = match image::load_from_memory(bytes) {
            Ok(image) => image.into_rgba8(),
            Err(error) => {
                eprintln!("Cannot read bundled Lucide icon {name}: {error}");
                return None;
            }
        };
        let pixels = egui::ColorImage::from_rgba_unmultiplied(
            [decoded.width() as usize, decoded.height() as usize],
            decoded.as_raw(),
        );
        let handle = ui
            .ctx()
            .load_texture(name, pixels, egui::TextureOptions::LINEAR);
        ui.ctx()
            .data_mut(|data| data.insert_temp(key, handle.clone()));
        Some(handle)
    });
    let color = if matches!(icon, Icon::Record) {
        ui.visuals().error_fg_color
    } else {
        ui.visuals().text_color()
    };
    let button = if let Some(texture) = texture {
        egui::Button::image(egui::Image::new((texture.id(), Vec2::splat(18.0))).tint(color))
    } else {
        egui::Button::new("Icon unavailable")
    };
    let button = if matches!(icon, Icon::Record) && active {
        button.fill(color.gamma_multiply(0.18))
    } else {
        button
    };
    button
        .min_size(Vec2::splat(30.0))
        .selected(active)
        .frame(active)
}
pub fn button(ui: &mut egui::Ui, icon: Icon, active: bool, help: &str) -> egui::Response {
    sized_button(ui, icon, active, help, 30.0)
}
pub fn toolbar_button(ui: &mut egui::Ui, icon: Icon, active: bool, help: &str) -> egui::Response {
    sized_button(ui, icon, active, help, 38.0)
}
fn sized_button(
    ui: &mut egui::Ui,
    icon: Icon,
    active: bool,
    help: &str,
    size: f32,
) -> egui::Response {
    let response = ui.add_sized(Vec2::splat(size), native_button(ui, icon, active));
    if matches!(icon, Icon::Grid | Icon::Rigs) {
        let center = response.rect.center();
        let stroke = egui::Stroke::new(1.6_f32, ui.visuals().text_color());
        for x in [-5.0, 5.0] {
            for y in [-5.0, 5.0] {
                if matches!(icon, Icon::Rigs) && x > 0.0 && y > 0.0 {
                    continue;
                }
                let cell =
                    egui::Rect::from_center_size(center + egui::vec2(x, y), Vec2::splat(7.0));
                ui.painter()
                    .rect_filled(cell, 1.0, ui.visuals().text_color().gamma_multiply(0.15));
                ui.painter()
                    .rect_stroke(cell, 1.0, stroke, egui::StrokeKind::Inside);
            }
        }
        if matches!(icon, Icon::Rigs) {
            let gear = center + egui::vec2(5.0, 5.0);
            ui.painter().circle_stroke(gear, 2.6, stroke);
            for direction in [
                egui::vec2(1.0, 0.0),
                egui::vec2(0.0, 1.0),
                egui::vec2(-1.0, 0.0),
                egui::vec2(0.0, -1.0),
            ] {
                ui.painter()
                    .line_segment([gear + direction * 2.6, gear + direction * 5.0], stroke);
            }
        }
    }
    if matches!(icon, Icon::Diagnostics) {
        let center = response.rect.center();
        let stroke = egui::Stroke::new(1.6_f32, ui.visuals().text_color());
        ui.painter().rect_stroke(
            egui::Rect::from_center_size(center + egui::vec2(0.0, 2.0), egui::vec2(9.0, 12.0)),
            4.5,
            stroke,
            egui::StrokeKind::Inside,
        );
        ui.painter()
            .circle_stroke(center + egui::vec2(0.0, -5.0), 2.5, stroke);
        ui.painter().line_segment(
            [
                center + egui::vec2(0.0, -2.0),
                center + egui::vec2(0.0, 7.0),
            ],
            stroke,
        );
        for side in [-1.0, 1.0] {
            ui.painter().line_segment(
                [
                    center + egui::vec2(side * 1.5, -7.0),
                    center + egui::vec2(side * 4.5, -10.0),
                ],
                stroke,
            );
            for y in [-2.0, 2.0, 6.0] {
                ui.painter().line_segment(
                    [
                        center + egui::vec2(side * 4.5, y),
                        center + egui::vec2(side * 9.0, y + (y - 2.0) * 0.3),
                    ],
                    stroke,
                );
            }
        }
    }
    if matches!(icon, Icon::Calibration) {
        let center = response.rect.center();
        let stroke = egui::Stroke::new(1.6_f32, ui.visuals().text_color());
        ui.painter().circle_stroke(center, 6.5, stroke);
        for direction in [
            egui::vec2(1.0, 0.0),
            egui::vec2(-1.0, 0.0),
            egui::vec2(0.0, 1.0),
            egui::vec2(0.0, -1.0),
        ] {
            ui.painter().line_segment(
                [center + direction * 4.0, center + direction * 10.0],
                stroke,
            );
        }
    }
    if matches!(icon, Icon::Link | Icon::Unlink) {
        // Native vector geometry, like the calibration reticle above. No
        // image download or decode is needed when opening camera controls.
        let center = response.rect.center();
        let stroke = egui::Stroke::new(1.5_f32, ui.visuals().text_color());
        let broken = matches!(icon, Icon::Unlink);
        for side in [-1.0_f32, 1.0] {
            let offset = if broken { 4.2 } else { 2.7 };
            let points = (0..=24)
                .map(|index| {
                    let angle = index as f32 / 24.0 * std::f32::consts::TAU;
                    let x = angle.cos() * 2.0;
                    let y = angle.sin() * 2.0 + if angle.sin() >= 0.0 { 2.2 } else { -2.2 };
                    center
                        + egui::vec2(
                            (x + y) * std::f32::consts::FRAC_1_SQRT_2 + side * offset,
                            (x - y) * std::f32::consts::FRAC_1_SQRT_2 - side * offset,
                        )
                })
                .collect();
            ui.painter().add(egui::Shape::line(points, stroke));
        }
        if broken {
            for side in [-1.0_f32, 1.0] {
                ui.painter().line_segment(
                    [
                        center + egui::vec2(side * 2.0, side * 3.5),
                        center + egui::vec2(side * 3.5, side * 5.0),
                    ],
                    stroke,
                );
            }
        }
    }
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), help));
    response.on_hover_ui(|ui| {
        let (title, description) = help.split_once('\n').unwrap_or((help, ""));
        ui.strong(title);
        if !description.is_empty() {
            ui.label(description);
        }
    })
}
pub fn menu<R>(
    ui: &mut egui::Ui,
    icon: Icon,
    help: &str,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    let (response, inner) =
        egui::containers::menu::MenuButton::from_button(native_button(ui, icon, false))
            .ui(ui, content);
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), help));
    egui::InnerResponse {
        response: response.on_hover_text(help),
        inner: inner.map(|r| r.inner),
    }
}

/// Larger target for the shared array recording action, using the same artwork.
pub fn record_all(ui: &mut egui::Ui, active: bool, help: &str) -> egui::Response {
    toolbar_button(ui, Icon::Record, active, help)
}

pub fn toolbar_menu<R>(
    ui: &mut egui::Ui,
    help: &str,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    let (response, inner) = egui::containers::menu::MenuButton::from_button(
        native_button(ui, Icon::More, false).min_size(Vec2::splat(38.0)),
    )
    .ui(ui, content);
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), help));
    egui::InnerResponse {
        response: response.on_hover_text(help),
        inner: inner.map(|r| r.inner),
    }
}
