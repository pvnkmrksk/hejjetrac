//! Bounded native-window timing, independent of numerical worker/publication FPS.
use std::{collections::VecDeque, time::Instant};

#[derive(Default)]
pub struct UiMetrics {
    last_frame: Option<(u64, Instant)>,
    intervals_ms: VecDeque<f64>,
    pub frames: u64,
    pub texture_updates: u64,
    pub show_ms: f64,
    pub update_ms: f64,
    pub textures_ms: f64,
    pub maximum_interval_ms: f64,
    pub source_rect: Option<[f32; 4]>,
    pub source_dimensions: Option<[usize; 2]>,
}

impl UiMetrics {
    pub fn begin(&mut self, frame: u64, now: Instant) {
        if self
            .last_frame
            .is_some_and(|(previous, _)| previous == frame)
        {
            return;
        }
        if let Some((_, previous)) = self.last_frame {
            let interval = now.saturating_duration_since(previous).as_secs_f64() * 1000.0;
            self.maximum_interval_ms = self.maximum_interval_ms.max(interval);
            self.intervals_ms.push_back(interval);
            if self.intervals_ms.len() > 240 {
                self.intervals_ms.pop_front();
            }
        }
        self.last_frame = Some((frame, now));
        self.frames = self.frames.saturating_add(1);
    }

    pub fn value(&self) -> serde_json::Value {
        let mut intervals: Vec<_> = self.intervals_ms.iter().copied().collect();
        intervals.sort_unstable_by(f64::total_cmp);
        let percentile = |fraction: f64| {
            intervals
                .get(((intervals.len() as f64 * fraction).ceil() as usize).saturating_sub(1))
                .copied()
        };
        serde_json::json!({
            "frames": self.frames,
            "texture_updates": self.texture_updates,
            "interval_ms_p50": percentile(0.5),
            "interval_ms_p95": percentile(0.95),
            "maximum_interval_ms": self.maximum_interval_ms,
            "show_ms": self.show_ms,
            "worker_snapshot_update_ms": self.update_ms,
            "texture_conversion_ms": self.textures_ms,
            "source_rect": self.source_rect,
            "source_dimensions": self.source_dimensions,
            "scope": "Native UI construction cadence; GPU presentation and display scanout are separate"
        })
    }
}
