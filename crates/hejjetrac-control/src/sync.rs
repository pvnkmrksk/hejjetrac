//! Recording synchronization evidence. These clocks never drive the estimator.
use crate::{Result, display};
use hejjetrac_core::TrackRow;
use hejjetrac_io::ConfigDocument;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RecordingMetadata {
    pub experiment_id: Option<String>,
    pub trial_id: Option<String>,
    pub trigger_id: Option<String>,
    /// Decimal UTC Unix nanoseconds supplied by the triggering computer.
    pub trigger_unix_ns: Option<String>,
    pub tags: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utc_estimates_retain_capture_offsets_and_worker_stop_before_finalization() -> Result<()> {
        let dir = std::env::temp_dir().join(format!(
            "hejjetrac-clock-test-{}-{}",
            std::process::id(),
            unix_ns()?
        ));
        std::fs::create_dir(&dir).map_err(display)?;
        let doc = ConfigDocument::parse("src_fn:0\n".into()).map_err(display)?;
        let mut log = SyncLog::create(&dir, &doc, &RecordingMetadata::default())?;
        let after = log.origin + std::time::Duration::from_millis(12);
        let before = log
            .origin
            .checked_sub(std::time::Duration::from_millis(8))
            .ok_or("test clock range")?;
        assert_eq!(
            log.estimated_unix_ns(after) - log.estimated_unix_ns(before),
            20_000_000
        );
        assert_eq!(log.elapsed_ns(before), -8_000_000);
        log.mark_stopped()?;
        let (_, stop_ns) = log.stop_observation.ok_or("missing stop observation")?;
        log.stop(3)?;
        log.stop(3)?;
        let events: Vec<serde_json::Value> =
            std::fs::read_to_string(dir.join("recording-events.jsonl"))
                .map_err(display)?
                .lines()
                .map(serde_json::from_str)
                .collect::<std::result::Result<_, _>>()
                .map_err(display)?;
        assert_eq!(events.len(), 2);
        assert_eq!(events[1]["observed_unix_ns"], stop_ns.to_string());
        assert_eq!(events[1]["epoch"], 3);
        drop(log);
        std::fs::remove_dir_all(dir).map_err(display)
    }
    #[test]
    fn malformed_or_unbounded_trial_metadata_is_rejected() {
        for args in [
            json!({"recording_metadata":{"trigger_unix_ns":3}}),
            json!({"recording_metadata":{"unknown":"value"}}),
            json!({"recording_metadata":{"trial_id":"bad\nline"}}),
            json!({"recording_metadata":{"tags":{"":"value"}}}),
            json!({"recording_metadata":{"trigger_unix_ns":"18446744073709551616"}}),
        ] {
            assert!(recording_metadata(&args).is_err());
        }
        let mut metadata = RecordingMetadata::default();
        for i in 0..33 {
            metadata.tags.insert(i.to_string(), "tag".into());
        }
        assert!(metadata.validate().is_err());
    }
}
impl RecordingMetadata {
    pub fn validate(&self) -> Result<()> {
        let valid = |s: &str| s.len() <= 1024 && !s.chars().any(char::is_control);
        if self.tags.len() > 32
            || self
                .tags
                .iter()
                .any(|(k, v)| k.is_empty() || k.len() > 128 || !valid(k) || !valid(v))
            || [&self.experiment_id, &self.trial_id, &self.trigger_id]
                .into_iter()
                .flatten()
                .any(|s| !valid(s))
        {
            return Err("Recording IDs/tag values must be at most 1024 bytes without control characters; at most 32 tags with 1..128-byte keys".into());
        }
        if self.trigger_unix_ns.as_ref().is_some_and(|s| {
            s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) || s.parse::<u64>().is_err()
        }) {
            return Err("trigger_unix_ns must be a decimal string containing UTC Unix nanoseconds within u64".into());
        }
        Ok(())
    }
}
pub fn recording_metadata(args: &serde_json::Value) -> Result<RecordingMetadata> {
    let metadata: RecordingMetadata = args
        .get("recording_metadata")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(display)?
        .unwrap_or_default();
    metadata.validate()?;
    Ok(metadata)
}
pub fn unix_ns() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(display)?
        .as_nanos()
        .try_into()
        .map_err(|_| "Host UTC clock exceeds the supported Unix nanosecond range".into())
}

#[derive(Clone, Copy)]
pub struct FrameTiming {
    pub delivered_at: Instant,
    /// Host-normalized capture-clock estimate, never an absolute exposure oracle.
    pub capture_at: Option<Instant>,
    pub source_sequence: Option<u64>,
    pub source_timestamp_ms: f64,
}
impl FrameTiming {
    pub fn replay(source_timestamp_ms: f64) -> Self {
        Self {
            delivered_at: Instant::now(),
            capture_at: None,
            source_sequence: None,
            source_timestamp_ms,
        }
    }
}

pub struct SyncLog {
    frames: BufWriter<File>,
    events: BufWriter<File>,
    origin: Instant,
    origin_unix_ns: u64,
    rows: u64,
    stopped: bool,
    stop_observation: Option<(Instant, u64)>,
}
impl SyncLog {
    pub fn create(
        directory: &Path,
        doc: &ConfigDocument,
        metadata: &RecordingMetadata,
    ) -> Result<Self> {
        metadata.validate()?;
        let before = Instant::now();
        let origin_unix_ns = unix_ns()?;
        let origin = Instant::now();
        let live = doc.get("src_fn").is_some_and(|s| s.parse::<u32>().is_ok());
        let manifest = json!({
            "schema_version":1,"application":"hejjetrac","version":env!("CARGO_PKG_VERSION"),
            "platform":std::env::consts::OS,"architecture":std::env::consts::ARCH,"pid":std::process::id(),
            "host":std::env::var("COMPUTERNAME").or_else(|_|std::env::var("HOSTNAME")).ok(),
            "instance_name":doc.get("rust_instance_name"),"source":doc.get("src_fn"),"camera_binding":doc.camera_binding().map_err(display)?,
            "source_kind":if live {"camera"} else {"recorded_video"},"recording_metadata":metadata,
            "clock_anchor_unix_ns":origin_unix_ns.to_string(),"anchor_sample_span_ns":origin.duration_since(before).as_nanos().to_string(),
            "files":{"frames":"frame-times.csv","events":"recording-events.jsonl"},
            "clock_semantics":"Unix nanoseconds are decimal strings in JSON. Delivery/capture estimates use this fixed UTC/monotonic anchor; observed_unix_ns samples the host UTC clock per row so clock adjustments remain visible. Capture estimates are normalized from the camera's relative capture clock and can include fixed delivery latency. Recorded-video capture UTC is unknown and left empty. Synchronize the tracking and VR computers' clocks (NTP/PTP); no network trigger or log establishes exposure-time accuracy or simultaneous camera starts."
        });
        let mut manifest_file = crate::create(&directory.join("recording.json"))?;
        serde_json::to_writer_pretty(&mut manifest_file, &manifest).map_err(display)?;
        manifest_file.sync_all().map_err(display)?;
        let mut frames = BufWriter::new(crate::create(&directory.join("frame-times.csv"))?);
        writeln!(frames, "recording_row,epoch,frame,source_sequence,source_timestamp_ms,timestamp_ms,delivery_unix_ns,capture_estimate_unix_ns,observed_unix_ns,monotonic_elapsed_ns,accepted,status").map_err(display)?;
        let mut log = Self {
            frames,
            events: BufWriter::new(crate::create(&directory.join("recording-events.jsonl"))?),
            origin,
            origin_unix_ns,
            rows: 0,
            stopped: false,
            stop_observation: None,
        };
        log.event("start", 0)?;
        Ok(log)
    }
    fn elapsed_ns(&self, at: Instant) -> i128 {
        if at >= self.origin {
            at.duration_since(self.origin).as_nanos() as i128
        } else {
            -(self.origin.duration_since(at).as_nanos() as i128)
        }
    }
    fn estimated_unix_ns(&self, at: Instant) -> i128 {
        i128::from(self.origin_unix_ns) + self.elapsed_ns(at)
    }
    pub fn event(&mut self, kind: &str, epoch: u32) -> Result<()> {
        self.event_at(kind, epoch, Instant::now(), unix_ns()?)
    }
    pub fn source(
        &mut self,
        label: &str,
        camera_id: Option<u32>,
        hardware_id: Option<&str>,
        port_id: Option<&str>,
    ) -> Result<()> {
        let event = json!({"event":"source","label":label,"camera_id":camera_id,"hardware_id":hardware_id,"port_id":port_id,"observed_unix_ns":unix_ns()?.to_string()});
        serde_json::to_writer(&mut self.events, &event).map_err(display)?;
        writeln!(self.events).map_err(display)?;
        self.events.flush().map_err(display)
    }
    fn event_at(&mut self, kind: &str, epoch: u32, now: Instant, observed: u64) -> Result<()> {
        let event = json!({"event":kind,"epoch":epoch,"recording_rows":self.rows,"observed_unix_ns":observed.to_string(),"monotonic_elapsed_ns":self.elapsed_ns(now).to_string()});
        serde_json::to_writer(&mut self.events, &event).map_err(display)?;
        writeln!(self.events).map_err(display)?;
        self.events.flush().map_err(display)
    }
    pub fn write(&mut self, epoch: u32, row: &TrackRow, timing: FrameTiming) -> Result<()> {
        if !timing.source_timestamp_ms.is_finite() || !row.timestamp_ms.is_finite() {
            return Err("Recording synchronization timestamps must be finite".into());
        }
        let observed = unix_ns()?;
        let delivery = self.estimated_unix_ns(timing.delivered_at);
        let capture = timing
            .capture_at
            .map(|at| self.estimated_unix_ns(at).to_string())
            .unwrap_or_default();
        let elapsed = self.elapsed_ns(timing.delivered_at);
        writeln!(
            self.frames,
            "{},{},{},{},{:.9},{:.9},{},{},{},{},{},{}",
            self.rows,
            epoch,
            row.frame,
            timing
                .source_sequence
                .map(|n| n.to_string())
                .unwrap_or_default(),
            timing.source_timestamp_ms,
            row.timestamp_ms,
            delivery,
            capture,
            observed,
            elapsed,
            row.accepted,
            row.status
        )
        .map_err(display)?;
        self.rows = self
            .rows
            .checked_add(1)
            .ok_or("Recording synchronization row counter overflow")?;
        Ok(())
    }
    pub fn flush(&mut self) -> Result<()> {
        self.frames.flush().map_err(display)?;
        self.events.flush().map_err(display)?;
        self.frames.get_ref().sync_all().map_err(display)?;
        self.events.get_ref().sync_all().map_err(display)
    }
    pub fn stop(&mut self, epoch: u32) -> Result<()> {
        if !self.stopped {
            self.mark_stopped()?;
            let (now, observed) = self
                .stop_observation
                .ok_or("Missing recording stop clock")?;
            self.event_at("stop", epoch, now, observed)?;
            self.stopped = true;
        }
        self.flush()
    }
    /// Capture the worker's Stop boundary before a background encoder drain.
    pub fn mark_stopped(&mut self) -> Result<()> {
        if self.stop_observation.is_none() {
            self.stop_observation = Some((Instant::now(), unix_ns()?));
        }
        Ok(())
    }
    pub fn cancel_stop(&mut self) {
        self.stop_observation = None;
    }
}
