use crate::{Error, Result};
use hejjetrac_core::{Config, Geometry, MapSettings, Sphere};
use std::{collections::BTreeMap, fs, path::Path};

pub struct LoadedConfig {
    pub config: Config,
    pub warnings: Vec<String>,
}
fn read(path: &Path) -> Result<String> {
    if fs::metadata(path)?.len() > 1_000_000 {
        return Err(Error::Config("config exceeds 1 MB".into()));
    }
    Ok(fs::read_to_string(path)?)
}
pub(crate) fn legacy(text: &str) -> Result<BTreeMap<String, String>> {
    let mut fields = BTreeMap::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let (k, v) = line
            .split_once(':')
            .ok_or_else(|| Error::Config(format!("expected key : value on line {}", i + 1)))?;
        if k.trim().is_empty()
            || k.chars()
                .chain(v.chars())
                .any(|c| c.is_control() && c != '\t')
        {
            return Err(Error::Config(format!(
                "invalid parameter name or control character on line {}",
                i + 1
            )));
        }
        if fields
            .insert(k.trim().to_owned(), v.trim().to_owned())
            .is_some()
        {
            return Err(Error::Config(format!("duplicate field {}", k.trim())));
        }
    }
    Ok(fields)
}
fn parse<T: serde::de::DeserializeOwned>(value: &str) -> Result<T> {
    Ok(serde_json::from_str(
        &value.replace('{', "[").replace('}', "]"),
    )?)
}
pub fn load_source_hint(path: &Path) -> Result<Option<String>> {
    let text = read(path)?;
    if text.trim_start().starts_with('{') {
        return Ok(modern_config(&text)?.config.source);
    }
    Ok(legacy(&text)?.get("src_fn").cloned())
}
pub fn load_config(path: &Path, width: usize, height: usize) -> Result<LoadedConfig> {
    let text = read(path)?;
    load_config_text(&text, width, height)
}
pub fn load_config_text(text: &str, width: usize, height: usize) -> Result<LoadedConfig> {
    if text.len() > 1_000_000 {
        return Err(Error::Config("config exceeds 1 MB".into()));
    }
    if text.trim_start().starts_with('{') {
        let loaded = modern_config(text)?;
        if loaded.config.width != width || loaded.config.height != height {
            return Err(Error::Config("video dimensions differ from config".into()));
        }
        return Ok(loaded);
    }
    crate::ConfigDocument::parse(text.to_owned())?.validate_frontend()?;
    let mut values = legacy(text)?;
    crate::zmq_output::ZmqSettings::parse(
        values.get("zmq_port").map(String::as_str),
        values.get("zmq_host").map(String::as_str),
        values.get("zmq_queue_capacity").map(String::as_str),
    )?;
    crate::ZmqPayloadSettings::parse(
        values.get("zmq_output_mode").map(String::as_str),
        values.get("zmq_framing").map(String::as_str),
        values.get("zmq_topic").map(String::as_str),
    )?;
    let mut config = Config {
        width,
        height,
        legacy_roi_coordinates: true,
        ..Config::default()
    };
    config.source = values.remove("src_fn");
    config.fisheye = values
        .remove("fisheye")
        .is_some_and(|v| matches!(v.as_str(), "y" | "Y" | "1" | "true"));
    config.vfov_deg = parse(
        &values
            .remove("vfov")
            .ok_or_else(|| Error::Config("legacy config requires vfov".into()))?,
    )?;
    if values.contains_key("roi_c") && values.contains_key("roi_r") {
        config.sphere = Sphere {
            centre: parse(&values.remove("roi_c").unwrap_or_default())?,
            angular_radius: parse(&values.remove("roi_r").unwrap_or_default())?,
        };
        values.remove("roi_circ");
    } else if let Some(points) = values.remove("roi_circ") {
        let coords: Vec<f64> = parse(&points)?;
        if !coords.len().is_multiple_of(2) {
            return Err(Error::Config("roi_circ requires x/y pairs".into()));
        }
        let points: Vec<_> = coords.as_chunks::<2>().0.to_vec();
        config.sphere =
            Geometry::fit_sphere_model(width, height, config.vfov_deg, &points, config.fisheye)?;
        values.remove("roi_c");
        values.remove("roi_r");
    } else {
        let centre = parse(
            &values
                .remove("roi_c")
                .ok_or_else(|| Error::Config("requires roi_circ or roi_c + roi_r".into()))?,
        )?;
        let angular_radius = parse(
            &values
                .remove("roi_r")
                .ok_or_else(|| Error::Config("missing roi_r".into()))?,
        )?;
        config.sphere = Sphere {
            centre,
            angular_radius,
        };
    }
    if let Some(v) = values.remove("c2a_r") {
        config.camera_to_lab = parse(&v)?;
    } else if let Some(source) = values.get("c2a_src") {
        let coords: Vec<f64> = parse(
            values
                .get(source)
                .ok_or_else(|| Error::Config(format!("missing {source} corner coordinates")))?,
        )?;
        if coords.len() != 8 {
            return Err(Error::Config(
                "lab calibration needs exactly four x/y corner pairs".into(),
            ));
        }
        let points = coords.as_chunks::<2>().0;
        let plane = source.strip_prefix("c2a_cnrs_").ok_or_else(|| {
            Error::Config(
                "c2a_src must select c2a_cnrs_xy, c2a_cnrs_yz or c2a_cnrs_xz when c2a_r is absent"
                    .into(),
            )
        })?;
        config.camera_to_lab = hejjetrac_core::calibration::lab_from_square(
            width,
            height,
            config.vfov_deg,
            config.fisheye,
            plane,
            points,
        )?
        .0;
    } else {
        return Err(Error::Config(
            "requires c2a_r or c2a_src with four square corners".into(),
        ));
    }
    if let Some(v) = values.remove("roi_ignr") {
        let polygons: Vec<Vec<f64>> = parse(&v)?;
        for polygon in polygons {
            if !polygon.len().is_multiple_of(2) {
                return Err(Error::Config("roi_ignr requires x/y pairs".into()));
            }
            config
                .ignore_polygons
                .push(polygon.as_chunks::<2>().0.to_vec());
        }
    }
    if let Some(v) = values.remove("opt_bound") {
        config.max_rotation = parse(&v)?;
    }
    if let Some(v) = values.get("src_fps") {
        let fps: f64 = parse(v)?;
        if !fps.is_finite() || fps > 10000.0 {
            return Err(Error::Config(
                "src_fps must be finite and at most 10000".into(),
            ));
        }
    }
    if let Some(v) = values.get("cam_model")
        && v != "rectilinear"
    {
        return Err(Error::Config(
            "only rectilinear cameras are supported".into(),
        ));
    }
    let spherical = values_engine(text)?;
    if spherical {
        let original = legacy(text)?;
        let number = |key: &str, default: f64| -> Result<f64> {
            match original.get(key) {
                Some(s) => parse(s),
                None => Ok(default),
            }
        };
        let defaults = MapSettings::default();
        let quality = number("q_factor", defaults.quality as f64)?;
        let evaluations = number("opt_max_evals", defaults.max_evaluations as f64)?;
        if quality.fract() != 0.0 || evaluations.fract() != 0.0 {
            return Err(Error::Config(
                "q_factor and opt_max_evals must be integers".into(),
            ));
        }
        let global_search = match original.get("opt_do_global").map(String::as_str) {
            None | Some("n" | "N" | "false" | "0") => false,
            Some("y" | "Y" | "true" | "1") => true,
            Some(_) => return Err(Error::Config("opt_do_global requires y or n".into())),
        };
        let max_bad_frames = original
            .get("max_bad_frames")
            .map(|v| parse::<i32>(v))
            .transpose()?
            .unwrap_or(-1);
        let global_evaluations = original
            .get("opt_global_evals")
            .map(|v| parse::<usize>(v))
            .transpose()?
            .unwrap_or(100000);
        config.spherical_map = Some(MapSettings {
            resilient: match original.get("rust_resilient").map(String::as_str) {
                None | Some("y" | "Y" | "true" | "1") => true,
                Some("n" | "N" | "false" | "0") => false,
                Some(_) => return Err(Error::Config("rust_resilient requires y or n".into())),
            },
            resilient_error_floor: number("rust_resilient_error_floor", 10_000.0)?,
            global_search,
            max_bad_frames,
            global_evaluations,
            channel: original.get("thr_rgb_tfrm").and_then(|s| match s.as_str() {
                "r" | "red" => Some(0),
                "g" | "green" => Some(1),
                "b" | "blue" => Some(2),
                _ => None,
            }),
            quality: quality as usize,
            threshold_ratio: number("thr_ratio", 1.25)?,
            threshold_window: number("thr_win_pc", 0.25)?,
            max_evaluations: evaluations as usize,
            tolerance: number("opt_tol", 0.001)?,
            max_error: number("opt_max_err", -1.0)?,
        });
        for key in [
            "q_factor",
            "thr_ratio",
            "thr_win_pc",
            "thr_rgb_tfrm",
            "opt_max_evals",
            "opt_tol",
            "opt_max_err",
            "opt_do_global",
            "rust_resilient",
            "rust_resilient_error_floor",
            "max_bad_frames",
            "opt_global_evals",
        ] {
            values.remove(key);
        }
    }
    values.remove("rust_engine");
    // These settings are executed by the common runtime/frontends, not the numerical core.
    for key in [
        "src_fps",
        "rust_instance_name",
        "rust_camera_width",
        "rust_camera_height",
        "rust_camera_id",
        "rust_calibration_size",
        "rust_calibration_vfov",
        "rust_calibration_fisheye",
        "rust_normalized_roi_circ",
        "rust_normalized_roi_ignr",
        "rust_normalized_c2a_cnrs_xy",
        "rust_normalized_c2a_cnrs_yz",
        "rust_normalized_c2a_cnrs_xz",
        "sphere_map_fn",
        "c2a_cnrs_xy",
        "c2a_cnrs_yz",
        "c2a_cnrs_xz",
        "c2a_src",
        "c2a_t",
        "do_display",
        "save_debug",
        "save_raw",
        "output_fn",
        "vid_codec",
        "sock_host",
        "sock_port",
        "zmq_host",
        "zmq_port",
        "zmq_queue_capacity",
        "zmq_output_mode",
        "zmq_framing",
        "zmq_topic",
        "com_port",
        "com_baud",
    ] {
        values.remove(key);
    }
    let warnings=values.keys().map(|k|format!("parameter '{k}' is retained for the frontend/compatibility; the numerical engine does not execute it")).collect();
    config.validate()?;
    Ok(LoadedConfig { config, warnings })
}

/// Unknown JSON settings are warnings at the file boundary. Known values retain
/// the strict typed deserializer and numerical validation before any state edit.
fn modern_config(text: &str) -> Result<LoadedConfig> {
    let mut parser = serde_json::Deserializer::from_str(text);
    let mut value = <UniqueJson as serde::Deserialize>::deserialize(&mut parser)?.0;
    parser.end()?;
    let defaults = serde_json::to_value(Config::default())?;
    let map_defaults = serde_json::to_value(MapSettings::default())?;
    let mut warnings = Vec::new();
    for (path, allowed) in [
        ("", &defaults),
        ("sphere", &defaults["sphere"]),
        ("spherical_map", &map_defaults),
    ] {
        let object = if path.is_empty() {
            value.as_object_mut()
        } else {
            value
                .get_mut(path)
                .and_then(serde_json::Value::as_object_mut)
        };
        if let Some(object) = object {
            object.retain(|key, _| {
                let known = allowed.get(key).is_some();
                if !known {
                    warnings.push(format!(
                        "unknown parameter '{}{key}' ignored",
                        if path.is_empty() {
                            String::new()
                        } else {
                            format!("{path}.")
                        }
                    ));
                }
                known
            });
        }
    }
    let config: Config = serde_json::from_value(value)?;
    config.validate()?;
    Ok(LoadedConfig { config, warnings })
}

// serde_json::Value ordinarily keeps the last duplicate key. Reject duplicates
// before filtering unknown settings so malformed hand edits remain unambiguous.
struct UniqueJson(serde_json::Value);
impl<'de> serde::Deserialize<'de> for UniqueJson {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = UniqueJson;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("JSON without duplicate object keys")
            }
            fn visit_bool<E: serde::de::Error>(
                self,
                value: bool,
            ) -> std::result::Result<UniqueJson, E> {
                Ok(UniqueJson(value.into()))
            }
            fn visit_i64<E: serde::de::Error>(
                self,
                value: i64,
            ) -> std::result::Result<UniqueJson, E> {
                Ok(UniqueJson(value.into()))
            }
            fn visit_u64<E: serde::de::Error>(
                self,
                value: u64,
            ) -> std::result::Result<UniqueJson, E> {
                Ok(UniqueJson(value.into()))
            }
            fn visit_f64<E: serde::de::Error>(
                self,
                value: f64,
            ) -> std::result::Result<UniqueJson, E> {
                serde_json::Number::from_f64(value)
                    .map(|n| UniqueJson(n.into()))
                    .ok_or_else(|| E::custom("nonfinite JSON number"))
            }
            fn visit_str<E: serde::de::Error>(
                self,
                value: &str,
            ) -> std::result::Result<UniqueJson, E> {
                Ok(UniqueJson(value.into()))
            }
            fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<UniqueJson, E> {
                Ok(UniqueJson(serde_json::Value::Null))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut access: A,
            ) -> std::result::Result<UniqueJson, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = access.next_element::<UniqueJson>()? {
                    values.push(value.0);
                }
                Ok(UniqueJson(values.into()))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut access: A,
            ) -> std::result::Result<UniqueJson, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = access.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom(format!("duplicate field {key:?}")));
                    }
                    values.insert(key, access.next_value::<UniqueJson>()?.0);
                }
                Ok(UniqueJson(values.into()))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

fn values_engine(text: &str) -> Result<bool> {
    match legacy(text)?.get("rust_engine").map(String::as_str) {
        None | Some("spherical") => Ok(true),
        Some("pairwise") => Ok(false),
        Some(other) => Err(Error::Config(format!("unknown rust_engine {other}"))),
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::*;
    #[test]
    fn unknown_json_settings_warn_while_known_bad_values_and_duplicates_are_rejected() -> Result<()>
    {
        let mut value = serde_json::to_value(Config::default())?;
        value["future_setting"] = serde_json::json!({"do_not_execute":"$(anything)"});
        value["sphere"]["future_setting"] = true.into();
        let loaded = load_config_text(&value.to_string(), 160, 160)?;
        assert_eq!(loaded.warnings.len(), 2);
        assert_eq!(loaded.config.width, 160);
        for bad in [
            serde_json::json!("NaN"),
            serde_json::json!(-1),
            serde_json::Value::Null,
        ] {
            value["width"] = bad;
            assert!(load_config_text(&value.to_string(), 160, 160).is_err());
        }
        for text in [
            r#"{"width":160,"width":160,"unknown":true}"#,
            r#"{"sphere":{"centre":[0,0,1],"centre":[0,0,1]}}"#,
            "{bad json",
            "{\"width\":1e999}",
        ] {
            assert!(load_config_text(text, 160, 160).is_err());
        }
        Ok(())
    }
}
