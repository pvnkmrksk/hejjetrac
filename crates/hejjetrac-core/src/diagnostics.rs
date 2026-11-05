//! FicTrac-shaped diagnostic views, produced from actual frames and accepted orientation.
use crate::{Config, Geometry, GrayFrame, Result, TrackRow};
use glam::{DQuat, DVec3};
use std::f64::consts::{PI, TAU};

// A cell must have a substantial accepted history before contradictory votes
// acquire extra inertia. At 100 FPS this requires at least ten seconds of
// actual visibility; newly learned surfaces keep upstream's consolidation.
const MATURE_TEACHING_FRAMES: u16 = 1024;
const REFINEMENT_CONFIRMATIONS: u8 = 8;
const REFINEMENT_VIEW_SEPARATION: f64 = 0.08;
const YOUNG_REFINEMENT_STEP: u8 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LearningPolicy {
    Frozen,
    NewSurface,
    Full,
    Guarded,
}
#[derive(Clone, Debug)]
pub struct DebugViews {
    pub roi: Vec<u8>,
    pub diff: Vec<u8>,
    pub instant: Vec<u8>,
    pub accumulated: Vec<u8>,
    pub roi_size: usize,
    pub map_width: usize,
    pub map_height: usize,
}
/// Independent consecutive-image support for an adaptive appearance decision.
#[derive(Clone, Debug, serde::Serialize)]
pub struct TemporalEvidence {
    pub known_samples: usize,
    pub possible_samples: usize,
    pub binary_mse: Option<f64>,
    pub spatially_supported_fraction: f64,
    pub verified: bool,
    /// Persistent atlas support and MSE expected from independent image/map
    /// marginals; a null match, not a replacement for reported fit MSE.
    pub persistent_known_samples: usize,
    pub persistent_chance_mse: Option<f64>,
}
pub struct Diagnostics {
    config: Config,
    pub(crate) basis: DQuat,
    pub(crate) points: Vec<Option<(DVec3, [f64; 2])>>,
    pub(crate) map: Vec<u8>,
    learning_hits: Vec<u16>,
    refinement_votes: Vec<u8>,
    last_refinement_frame: Vec<u32>,
    last_refinement_view: Vec<u32>,
    refinement_view: u32,
    refinement_pose: Option<DQuat>,
    projected_sites: Vec<usize>,
    refinement_support: Vec<bool>,
    cell_votes: Vec<u8>,
    candidate_registered: Vec<bool>,
    candidate_back_seen: Vec<bool>,
    candidate_points: Vec<(usize, DVec3)>,
    last_learning_epoch: Vec<u32>,
    learning_epoch: u32,
    resilient_learning: bool,
    previous: Option<GrayFrame>,
    previous_accepted: bool,
    pub(crate) size: usize,
    pub(crate) map_width: usize,
    pub(crate) map_height: usize,
    ratio: f64,
    window: usize,
}
impl Diagnostics {
    pub fn new(config: Config, quality: usize, ratio: f64, window_fraction: f64) -> Result<Self> {
        let g = Geometry::new(config.clone())?;
        let basis = DQuat::from_rotation_arc(DVec3::from_array(config.sphere.centre), DVec3::Z);
        let size = (quality.clamp(1, 30) * 10).min(config.width);
        // Upstream clamps ROI width to source width, then derives the even-width atlas.
        let map_height = size * 3 / 2;
        let map_width = map_height * 2;
        let radius = config.sphere.angular_radius;
        let mut points = vec![];
        for y in 0..size {
            for x in 0..size {
                let dx = (x as f64 + 0.5 - size as f64 * 0.5) * 2.0 * radius / size as f64;
                let dy = (y as f64 + 0.5 - size as f64 * 0.5) * 2.0 * radius / size as f64;
                let theta = dx.hypot(dy);
                let factor = if theta > 1e-10 {
                    theta.sin() / theta
                } else {
                    1.0
                };
                let ray = basis.inverse() * DVec3::new(dx * factor, dy * factor, theta.cos());
                let xy = g
                    .project_ray(ray)
                    .map(|p| [p[0] - 0.5, p[1] - 0.5])
                    .unwrap_or([-1.0, -1.0]);
                let p = if theta < radius * 0.975 && g.usable(xy[0], xy[1]) {
                    let c = DVec3::from_array(config.sphere.centre);
                    let dot = ray.dot(c);
                    let r = config.sphere.angular_radius.sin();
                    let disc = dot * dot + r * r - 1.0;
                    (disc > 0.0).then(|| ((ray * (dot - disc.sqrt()) - c) / r, xy))
                } else {
                    None
                };
                points.push(p);
            }
        }
        let window =
            ((window_fraction.clamp(0.0, 1.0) * size as f64).round() as usize | 1).min(size | 1);
        let resilient_learning = config.spherical_map.as_ref().is_some_and(|m| m.resilient);
        Ok(Self {
            config,
            basis,
            points,
            map: vec![128; map_width * map_height],
            learning_hits: vec![0; map_width * map_height],
            refinement_votes: vec![0; map_width * map_height],
            last_refinement_frame: vec![0; map_width * map_height],
            last_refinement_view: vec![0; map_width * map_height],
            refinement_view: 0,
            refinement_pose: None,
            projected_sites: vec![0; size * size],
            refinement_support: vec![false; size * size],
            cell_votes: vec![0; map_width * map_height],
            candidate_registered: vec![false; map_width * map_height],
            candidate_back_seen: vec![false; map_width * map_height],
            candidate_points: Vec::with_capacity(map_width * map_height),
            last_learning_epoch: vec![0; map_width * map_height],
            learning_epoch: 0,
            resilient_learning,
            previous: None,
            previous_accepted: false,
            size,
            map_width,
            map_height,
            ratio: ratio.clamp(0.01, 100.0),
            window,
        })
    }
    pub(crate) fn reset_history(&mut self) {
        self.previous = None;
        self.previous_accepted = false;
        self.learning_hits.fill(0);
        self.refinement_votes.fill(0);
        self.last_refinement_frame.fill(0);
        self.last_refinement_view.fill(0);
        self.refinement_view = 0;
        self.refinement_pose = None;
        self.candidate_registered.fill(false);
        self.candidate_back_seen.fill(false);
        self.candidate_points.clear();
        self.last_learning_epoch.fill(0);
        self.learning_epoch = 0;
    }
    fn begin_learning_frame(&mut self) {
        self.learning_epoch = if let Some(next) = self.learning_epoch.checked_add(1) {
            next
        } else {
            self.last_learning_epoch.fill(0);
            self.last_refinement_frame.fill(0);
            1
        };
    }
    fn begin_refinement_view(&mut self, pose: DQuat) {
        let view = self.basis * (pose.inverse() * DVec3::from_array(self.config.sphere.centre));
        for &(index, normal) in &self.candidate_points {
            if self.refinement_votes[index] > 0 && normal.dot(view) > 0.0 {
                self.candidate_back_seen[index] = true;
            }
        }
        if self.refinement_pose.is_none_or(|previous| {
            previous.dot(pose).abs() <= (REFINEMENT_VIEW_SEPARATION * 0.5).cos()
        }) {
            self.refinement_view = if let Some(next) = self.refinement_view.checked_add(1) {
                next
            } else {
                self.last_refinement_view.fill(0);
                1
            };
            self.refinement_pose = Some(pose);
        }
    }
    fn learn_site(
        &mut self,
        index: usize,
        observation: u8,
        refinement_allowed: bool,
        revisited: bool,
    ) {
        let first_hit = self.learning_hits[index] == 0;
        let new_refinement_frame = self.last_refinement_frame[index] != self.learning_epoch;
        let mut mature = false;
        if self.resilient_learning {
            if self.last_learning_epoch[index] != self.learning_epoch {
                self.learning_hits[index] = if self.learning_hits[index] == u16::MAX {
                    // Retain mature inertia while preserving its modulo-16
                    // update phase beyond 65535 observations. Saturating on an
                    // ineligible remainder would freeze this cell forever.
                    u16::MAX - 15
                } else {
                    self.learning_hits[index] + 1
                };
                self.last_learning_epoch[index] = self.learning_epoch;
            }
            // Count distinct teaching frames, not colliding ROI samples. New
            // surface retains its original first-hit bonus and early learning;
            // contradictory evidence against a long-observed consolidated
            // cell acquires sixteen times the inertia. Agreement consolidates fast;
            // near-neutral evidence remains free to resolve either way.
            let hits = self.learning_hits[index];
            let period = (1 + hits.saturating_sub(1) / 64).min(16);
            mature = hits >= MATURE_TEACHING_FRAMES;
            let contrary = (self.map[index] >= 160 && observation == 0)
                || (self.map[index] <= 96 && observation == 255);
            let refining = self.map[index] == 0
                || self.map[index] == 255
                || self.refinement_votes[index] >= REFINEMENT_CONFIRMATIONS;
            if refining && !refinement_allowed {
                return;
            }
            if mature && !first_hit && contrary && !hits.is_multiple_of(period) {
                return;
            }
            // Fully consolidated cells remain revisable, but a boundary vote
            // or several colliding samples from one image cannot unlock them.
            // Require repeated contrary teaching frames before consolidation
            // can move away from an extreme. Mature cells keep their slower
            // update phase as well.
            if self.map[index] == 0 || self.map[index] == 255 {
                if self.map[index] == observation {
                    self.refinement_votes[index] = 0;
                    self.candidate_back_seen[index] = false;
                } else {
                    if new_refinement_frame {
                        if !self.candidate_registered[index] {
                            let y = (index / self.map_width) as f64;
                            let x = (index % self.map_width) as f64;
                            let py = (y + 0.5) * 2.0 / self.map_height as f64 - 1.0;
                            let longitude = -(x + 0.5) / self.map_width as f64 * TAU - PI;
                            let radius = (1.0 - py * py).max(0.0).sqrt();
                            self.candidate_points.push((
                                index,
                                DVec3::new(radius * longitude.sin(), py, radius * longitude.cos()),
                            ));
                            self.candidate_registered[index] = true;
                        }
                        self.refinement_votes[index] =
                            self.refinement_votes[index].saturating_add(1);
                        self.last_refinement_frame[index] = self.learning_epoch;
                        self.last_refinement_view[index] = self.refinement_view;
                    }
                    // A run of views in the first sweep can share an alias or
                    // transient obstruction. Keep its anchor until this
                    // material patch has faced away and is observed again.
                    if self.refinement_votes[index] < REFINEMENT_CONFIRMATIONS || !revisited {
                        return;
                    }
                }
            }
            if self.refinement_votes[index] >= REFINEMENT_CONFIRMATIONS && !new_refinement_frame {
                // Once unlocked, revise at most once per teaching frame.
                return;
            }
            if self.refinement_votes[index] >= REFINEMENT_CONFIRMATIONS {
                self.last_refinement_frame[index] = self.learning_epoch;
            }
        }
        let step = if self.resilient_learning
            && !mature
            && self.refinement_votes[index] >= REFINEMENT_CONFIRMATIONS
        {
            YOUNG_REFINEMENT_STEP
        } else {
            1
        };
        self.map[index] = match self.map[index] {
            0 if !self.resilient_learning => 0,
            255 if !self.resilient_learning => 255,
            128 if !mature || first_hit => {
                if observation == 255 {
                    192
                } else {
                    64
                }
            }
            value => {
                if observation == 255 {
                    value.saturating_add(step)
                } else {
                    value.saturating_sub(step)
                }
            }
        };
    }
    fn teach_site(
        &mut self,
        index: usize,
        observation: u8,
        policy: LearningPolicy,
        refinement_allowed: bool,
        revisited: bool,
    ) {
        if matches!(policy, LearningPolicy::Full | LearningPolicy::Guarded)
            || (policy == LearningPolicy::NewSurface
                && self.map[index] == 128
                && self.learning_hits[index] == 0)
        {
            // A previously observed cell can pass through neutral 128; it is
            // not new surface. Imported templates use 128 for unseen cells,
            // while their nonneutral confidence values remain protected even
            // though imports deliberately reset age history.
            self.learn_site(index, observation, refinement_allowed, revisited);
        }
    }
    pub(crate) fn prepare(&self, frame: &GrayFrame) -> Vec<u8> {
        let n = self.size;
        let grey: Vec<_> = self
            .points
            .iter()
            .map(|p| {
                p.and_then(|(_, xy)| frame.sample(xy[0], xy[1]))
                    .map(|v| (v * 255.0).round() as u8)
                    .unwrap_or(128)
            })
            .collect();
        let mut blur = grey.clone();
        for y in 0..n {
            for x in 0..n {
                if x > 0 && x + 1 < n && y > 0 && y + 1 < n {
                    blur[y * n + x] = median9([
                        grey[(y - 1) * n + x - 1],
                        grey[(y - 1) * n + x],
                        grey[(y - 1) * n + x + 1],
                        grey[y * n + x - 1],
                        grey[y * n + x],
                        grey[y * n + x + 1],
                        grey[(y + 1) * n + x - 1],
                        grey[(y + 1) * n + x],
                        grey[(y + 1) * n + x + 1],
                    ]);
                    continue;
                }
                let mut values = [0u8; 9];
                let mut k = 0;
                for yy in y.saturating_sub(1)..=(y + 1).min(n - 1) {
                    for xx in x.saturating_sub(1)..=(x + 1).min(n - 1) {
                        values[k] = grey[yy * n + xx];
                        k += 1;
                    }
                }
                values[..k].sort_unstable();
                blur[y * n + x] = values[k / 2];
            }
        }
        let rad = self.window / 2;
        let mut horizontal = vec![Extrema::EMPTY; n * n];
        let mut prefix = vec![Extrema::EMPTY; n + 2 * rad];
        let mut suffix = prefix.clone();
        for y in 0..n {
            extrema_windows(
                n,
                rad,
                &mut prefix,
                &mut suffix,
                |x| {
                    let i = y * n + x;
                    if self.points[i].is_none() {
                        Extrema::EMPTY
                    } else {
                        Extrema {
                            min: blur[i],
                            max: if blur[i] < 255 { blur[i] } else { 0 },
                        }
                    }
                },
                |x, value| horizontal[y * n + x] = value,
            );
        }
        let mut roi = vec![128; n * n];
        for x in 0..n {
            extrema_windows(
                n,
                rad,
                &mut prefix,
                &mut suffix,
                |y| horizontal[y * n + x],
                |y, bounds| {
                    let i = y * n + x;
                    if self.points[i].is_none() {
                        return;
                    }
                    let value = if self.ratio * (f64::from(grey[i]) - f64::from(bounds.min))
                        <= f64::from(bounds.max) - f64::from(grey[i])
                    {
                        0
                    } else {
                        255
                    };
                    roi[i] = value;
                },
            );
        }
        roi
    }
    /// Uniform regions do not independently identify material surface. Leave
    /// new cells there unknown until a visible pattern supports their seed.
    pub(crate) fn informative_sites(&self, roi: &[u8]) -> Vec<bool> {
        let n = self.size;
        // Broad legitimate markings must retain support at the default small
        // ROI. A tiny neighborhood confuses their interiors with an occluder
        // and can leave a clean, low-resolution atlas too sparse to fit.
        let radius = (n / 8).max(1);
        (0..roi.len())
            .map(|index| {
                if roi[index] == 128 {
                    return false;
                }
                let (x, y) = (index % n, index / n);
                let mut black = 0usize;
                let mut white = 0usize;
                let mut count = |value| {
                    black += usize::from(value == 0);
                    white += usize::from(value == 255);
                };
                for xx in x.saturating_sub(radius)..=(x + radius).min(n - 1) {
                    count(roi[y * n + xx]);
                }
                for yy in y.saturating_sub(radius)..=(y + radius).min(n - 1) {
                    if yy != y {
                        count(roi[yy * n + x]);
                    }
                }
                black >= 2 && white >= 2
            })
            .collect()
    }
    pub fn update(&mut self, frame: &GrayFrame, row: Option<&TrackRow>) -> DebugViews {
        let roi = self.prepare(frame);
        self.update_prepared(frame, row, roi, LearningPolicy::Full)
    }
    /// The atlas estimator and its committed map must use the same thresholded
    /// observation. Reuse it rather than sampling/filtering the image twice.
    pub(crate) fn update_prepared(
        &mut self,
        frame: &GrayFrame,
        row: Option<&TrackRow>,
        roi: Vec<u8>,
        learning: LearningPolicy,
    ) -> DebugViews {
        let n = self.size;
        let mw = self.map_width;
        let mh = self.map_height;
        let mut diff = vec![0; n * n];
        let mut instant = vec![128; mw * mh];
        let orientation = row
            .map(|r| DQuat::from_scaled_axis(DVec3::from_array(r.absolute_camera)))
            .unwrap_or(DQuat::IDENTITY);
        let exported_basis = if self.config.legacy_roi_coordinates {
            self.basis
        } else {
            DQuat::IDENTITY
        };
        let orientation_cam = exported_basis.inverse() * orientation * exported_basis;
        let rotation = row
            .map(|r| exported_basis.inverse() * DVec3::from_array(r.delta_camera))
            .unwrap_or(DVec3::ZERO);
        let inverse_delta = DQuat::from_scaled_axis(-rotation);
        // Fast views can share the same exposure smear across many images.
        // Revisit established labels only below one atlas-longitude step;
        // faster accepted views can still extend genuinely unseen surface.
        let refinement_motion_ok = rotation.length() <= TAU / mw as f64;
        let learning = if row.is_none_or(|r| r.accepted) {
            learning
        } else {
            LearningPolicy::Frozen
        };
        if learning != LearningPolicy::Frozen && self.resilient_learning {
            self.begin_learning_frame();
            self.begin_refinement_view(orientation_cam);
        }
        let informative = matches!(
            learning,
            LearningPolicy::Guarded | LearningPolicy::NewSurface
        )
        .then(|| self.informative_sites(&roi))
        .filter(|sites| {
            // A predominantly supported image can teach the ordinary interiors
            // of its broad markings. Restricting every first hit to an edge
            // leaves holes even in a clean ball and destabilizes fast fitting.
            // Low-support partial views and acceleration probation still need
            // independent local support for every newly seeded cell.
            learning == LearningPolicy::NewSurface
                || sites.iter().filter(|site| **site).count() * 2
                    < self.points.iter().filter(|point| point.is_some()).count()
        });
        self.cell_votes.fill(0);
        for y in 0..n {
            for x in 0..n {
                let i = y * n + x;
                let Some((p, _)) = self.points[i] else {
                    continue;
                };
                let value = roi[i];
                let previous_point = inverse_delta * p;
                let ray = self.basis
                    * (DVec3::from_array(self.config.sphere.centre)
                        + previous_point * self.config.sphere.angular_radius.sin());
                let radial = ray.x.hypot(ray.y);
                let factor = if radial > 1e-12 {
                    radial.atan2(ray.z) * n as f64
                        / (2.0 * self.config.sphere.angular_radius * radial)
                } else {
                    n as f64 / (2.0 * self.config.sphere.angular_radius * ray.z)
                };
                let xy = [
                    ray.x * factor + n as f64 * 0.5 - 0.5,
                    ray.y * factor + n as f64 * 0.5 - 0.5,
                ];
                let previous = self.previous.as_ref().and_then(|p| p.sample(xy[0], xy[1]));
                if let Some(v) = previous {
                    diff[i] = (f64::from(value) - v * 255.0).abs().min(255.0) as u8;
                }
                let p = self.basis * (orientation_cam.inverse() * p);
                let lon = p.x.atan2(p.z);
                let mx = ((-lon - PI) / TAU).rem_euclid(1.0) * mw as f64;
                let my = (p.y + 1.0) * 0.5 * mh as f64;
                let mi = (my.floor() as usize).min(mh - 1) * mw + (mx.floor() as usize).min(mw - 1);
                instant[mi] = value;
                self.projected_sites[i] = mi;
                self.cell_votes[mi] |= if value == 0 { 1 } else { 2 };
                // Revisit a consolidated marking only in its supported
                // interior, with matching evidence from the preceding
                // accepted observation warped by the measured increment.
                // Threshold boundaries and single-image defects cannot
                // rewrite a confident material label.
                self.refinement_support[i] = refinement_motion_ok
                    && self.previous_accepted
                    && previous
                        .is_some_and(|v| (value == 0 && v <= 0.1) || (value == 255 && v >= 0.9))
                    && x > 0
                    && x + 1 < n
                    && y > 0
                    && y + 1 < n
                    && [i - 1, i + 1, i - n, i + n]
                        .iter()
                        .filter(|neighbor| roi[**neighbor] == value)
                        .count()
                        >= 3;
            }
        }
        // Several ROI sites can land in one atlas cell. An early colliding
        // sample must not overturn the label when other current samples
        // disagree. Resolve that ambiguity before applying any refinement,
        // while preserving the original order of ordinary confidence updates.
        for (i, value) in roi.iter().copied().enumerate() {
            if self.points[i].is_none() {
                continue;
            }
            let mi = self.projected_sites[i];
            let unobserved = self.map[mi] == 128 && self.learning_hits[mi] == 0;
            if !unobserved || informative.as_ref().is_none_or(|sites| sites[i]) {
                let refinement_allowed = self.refinement_support[i]
                    && self.cell_votes[mi] != 3
                    && (self.map[mi] != 0 && self.map[mi] != 255
                        || self.map[mi] == value
                        || self.last_refinement_view[mi] != self.refinement_view);
                self.teach_site(
                    mi,
                    value,
                    learning,
                    refinement_allowed,
                    self.candidate_back_seen[mi],
                );
            }
        }
        self.previous = GrayFrame::new(
            n,
            n,
            roi.iter().map(|v| f32::from(*v) / 255.0).collect(),
            frame.timestamp_ms(),
        )
        .ok();
        self.previous_accepted = row.is_none_or(|row| row.accepted);
        DebugViews {
            roi,
            diff,
            instant,
            accumulated: self.map.clone(),
            roi_size: n,
            map_width: mw,
            map_height: mh,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Extrema {
    min: u8,
    max: u8,
}
impl Extrema {
    const EMPTY: Self = Self { min: 255, max: 0 };
    fn join(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }
}
/// Prefix/suffix blocks give each clipped window in constant work, independent
/// of its radius. Padding with the identity preserves masked-border semantics.
fn extrema_windows(
    n: usize,
    rad: usize,
    prefix: &mut [Extrema],
    suffix: &mut [Extrema],
    value: impl Fn(usize) -> Extrema,
    mut emit: impl FnMut(usize, Extrema),
) {
    let extent = n + 2 * rad;
    let window = 2 * rad + 1;
    let sample = |i: usize| {
        if i >= rad && i - rad < n {
            value(i - rad)
        } else {
            Extrema::EMPTY
        }
    };
    for i in 0..extent {
        prefix[i] = if i % window == 0 {
            sample(i)
        } else {
            prefix[i - 1].join(sample(i))
        };
    }
    for i in (0..extent).rev() {
        suffix[i] = if i + 1 == extent || (i + 1) % window == 0 {
            sample(i)
        } else {
            suffix[i + 1].join(sample(i))
        };
    }
    for i in 0..n {
        emit(i, suffix[i].join(prefix[i + 2 * rad]));
    }
}
/// Median selection network: only the fifth order statistic is needed.
/// The 512 binary inputs exhaust its threshold cases (zero-one principle).
fn median9(mut p: [u8; 9]) -> u8 {
    for (a, b) in [
        (1, 2),
        (4, 5),
        (7, 8),
        (0, 1),
        (3, 4),
        (6, 7),
        (1, 2),
        (4, 5),
        (7, 8),
        (0, 3),
        (5, 8),
        (4, 7),
        (3, 6),
        (1, 4),
        (2, 5),
        (4, 7),
        (4, 2),
        (6, 4),
        (4, 2),
    ] {
        let lo = p[a].min(p[b]);
        let hi = p[a].max(p[b]);
        p[a] = lo;
        p[b] = hi;
    }
    p[4]
}

#[cfg(test)]
mod learning_tests {
    use super::*;
    use crate::{MapSettings, Tracker};

    fn diagnostics(resilient: bool) -> Result<Diagnostics> {
        Diagnostics::new(
            Config {
                spherical_map: Some(MapSettings {
                    resilient,
                    ..MapSettings::default()
                }),
                ..Config::default()
            },
            6,
            1.25,
            0.25,
        )
    }

    #[test]
    fn repeated_accepted_passes_can_correct_saturated_first_pass_evidence() -> Result<()> {
        let mut adaptive = diagnostics(true)?;
        let mut legacy = diagnostics(false)?;
        for map in [&mut adaptive, &mut legacy] {
            for _ in 0..80 {
                map.begin_learning_frame();
                map.learn_site(0, 255, true, true);
                map.learn_site(1, 0, true, true);
            }
            assert_eq!(&map.map[..2], &[255, 0]);
        }
        // Later accepted views consistently contradict the first pass. A single
        // image must not flip its labels, but sustained evidence must correct
        // both extremes instead of treating them as permanent material truth.
        for pass in 0..300 {
            for map in [&mut adaptive, &mut legacy] {
                map.begin_learning_frame();
                map.teach_site(0, 0, LearningPolicy::Guarded, true, true);
                map.teach_site(1, 255, LearningPolicy::Guarded, true, true);
            }
            if pass == 0 {
                assert!(adaptive.map[0] > 128 && adaptive.map[1] < 128);
            }
        }
        assert!(adaptive.map[0] < 128 && adaptive.map[1] > 128);
        assert_eq!(&legacy.map[..2], &[255, 0], "legacy opt-out stays fixed");
        Ok(())
    }

    #[test]
    fn saturated_refinement_needs_distinct_frames_and_resists_transient_votes() -> Result<()> {
        let mut adaptive = diagnostics(true)?;
        adaptive.map[0] = 255;
        for _ in 0..REFINEMENT_CONFIRMATIONS - 1 {
            adaptive.begin_learning_frame();
            for _ in 0..100 {
                adaptive.learn_site(0, 0, true, true);
            }
            assert_eq!(adaptive.map[0], 255, "one image cannot unlock a cell");
        }
        adaptive.begin_learning_frame();
        adaptive.learn_site(0, 255, true, true);
        assert_eq!(
            adaptive.refinement_votes[0], 0,
            "agreement clears a transient streak"
        );
        for _ in 0..REFINEMENT_CONFIRMATIONS {
            adaptive.begin_learning_frame();
            for _ in 0..100 {
                adaptive.learn_site(0, 0, true, true);
            }
        }
        assert_eq!(
            adaptive.map[0],
            255 - YOUNG_REFINEMENT_STEP,
            "refinement counts each image once"
        );
        adaptive.reset_history();
        assert!(adaptive.refinement_votes.iter().all(|votes| *votes == 0));
        Ok(())
    }

    #[test]
    fn mature_saturated_cells_remain_protected_but_can_eventually_refine() -> Result<()> {
        let mut adaptive = diagnostics(true)?;
        adaptive.map[0] = 255;
        adaptive.map[1] = 0;
        adaptive.learning_hits[..2].fill(1600);
        for image in 0..3000 {
            adaptive.begin_learning_frame();
            adaptive.teach_site(0, 0, LearningPolicy::Guarded, true, true);
            adaptive.teach_site(1, 255, LearningPolicy::Guarded, true, true);
            if image == 299 {
                assert!(adaptive.map[0] >= 160 && adaptive.map[1] <= 96);
            }
        }
        assert!(adaptive.map[0] < 128 && adaptive.map[1] > 128);
        Ok(())
    }

    #[test]
    fn boundaries_and_unaccepted_previous_images_cannot_unlock_confident_cells() -> Result<()> {
        let mut adaptive = diagnostics(true)?;
        adaptive.map.fill(255);
        let base = GrayFrame::new(160, 160, vec![0.5; 160 * 160], 0.)?;
        let mut row = Tracker::new(Config::default())?.process(base.clone())?;
        let boundary: Vec<_> = (0..adaptive.size * adaptive.size)
            .map(|i| {
                if (i / adaptive.size + i % adaptive.size) % 2 == 0 {
                    0
                } else {
                    255
                }
            })
            .collect();
        for i in 0..100 {
            row.absolute_camera = [i as f64 * 0.02, 0., 0.];
            row.delta_camera = [0.02, 0., 0.];
            adaptive.update_prepared(&base, Some(&row), boundary.clone(), LearningPolicy::Guarded);
        }
        assert!(adaptive.map.iter().all(|value| *value == 255));
        assert!(adaptive.refinement_votes.iter().all(|votes| *votes == 0));
        let uniform = vec![0; adaptive.size * adaptive.size];
        for i in 100..200 {
            row.absolute_camera = [i as f64 * 0.02, 0., 0.];
            row.accepted = false;
            adaptive.update_prepared(&base, Some(&row), uniform.clone(), LearningPolicy::Guarded);
            row.accepted = true;
            adaptive.update_prepared(&base, Some(&row), uniform.clone(), LearningPolicy::Guarded);
        }
        assert!(adaptive.map.iter().all(|value| *value == 255));
        assert!(adaptive.refinement_votes.iter().all(|votes| *votes == 0));
        Ok(())
    }

    #[test]
    fn confidence_revision_requires_separated_slow_views() -> Result<()> {
        let mut adaptive = diagnostics(true)?;
        adaptive.map.fill(255);
        let base = GrayFrame::new(160, 160, vec![0.5; 160 * 160], 0.)?;
        let mut row = Tracker::new(Config::default())?.process(base.clone())?;
        let uniform = vec![0; adaptive.size * adaptive.size];
        for _ in 0..100 {
            adaptive.update_prepared(&base, Some(&row), uniform.clone(), LearningPolicy::Guarded);
        }
        assert!(adaptive.map.iter().all(|value| *value == 255));
        assert!(adaptive.refinement_votes.iter().all(|votes| *votes <= 1));
        row.delta_camera = [0.02, 0., 0.];
        for i in 1..=100 {
            row.absolute_camera = [i as f64 * 0.02, 0., 0.];
            adaptive.update_prepared(&base, Some(&row), uniform.clone(), LearningPolicy::Guarded);
        }
        assert!(
            adaptive.map.iter().all(|value| *value == 255),
            "a first pass is not an independent revisit"
        );
        row.delta_camera = [0.2, 0., 0.];
        for i in 1..=100 {
            row.absolute_camera = [2.0 + i as f64 * 0.2, 0., 0.];
            adaptive.update_prepared(&base, Some(&row), uniform.clone(), LearningPolicy::Guarded);
        }
        assert!(adaptive.map.iter().all(|value| *value == 255));
        row.delta_camera = [0.02, 0., 0.];
        for i in 1..=400 {
            row.absolute_camera = [22.0 + i as f64 * 0.02, 0., 0.];
            adaptive.update_prepared(&base, Some(&row), uniform.clone(), LearningPolicy::Guarded);
        }
        assert!(adaptive.map.iter().any(|value| *value < 128));
        Ok(())
    }

    #[test]
    fn conflicting_projections_cannot_confirm_a_material_label() -> Result<()> {
        let mut adaptive = diagnostics(true)?;
        adaptive.map.fill(255);
        let base = GrayFrame::new(160, 160, vec![0.5; 160 * 160], 0.)?;
        let row = Tracker::new(Config::default())?.process(base.clone())?;
        let roi: Vec<_> = (0..adaptive.size * adaptive.size)
            .map(|i| {
                if i % adaptive.size < adaptive.size / 2 + 2 {
                    0
                } else {
                    255
                }
            })
            .collect();
        for _ in 0..2 {
            adaptive.update_prepared(&base, Some(&row), roi.clone(), LearningPolicy::Guarded);
        }
        let mixed: Vec<_> = adaptive
            .cell_votes
            .iter()
            .enumerate()
            .filter_map(|(index, votes)| (*votes == 3).then_some(index))
            .collect();
        assert!(
            !mixed.is_empty(),
            "fixture must project both colors into shared cells"
        );
        assert!(
            mixed
                .iter()
                .all(|index| adaptive.refinement_votes[*index] == 0)
        );
        assert!(
            adaptive.refinement_votes.contains(&1),
            "unambiguous cells can supply evidence"
        );
        Ok(())
    }

    #[test]
    fn mature_unsaturated_map_resists_300_accepted_contrary_images() -> Result<()> {
        let mut protected = diagnostics(true)?;
        let mut legacy = diagnostics(false)?;
        // A long-observed boundary cell can remain unsaturated. Exercise that
        // vulnerable evidence as well as the fully consolidated extremes.
        protected.map.fill(200);
        legacy.map.fill(200);
        protected.learning_hits.fill(1600);
        let base = GrayFrame::new(160, 160, vec![0.5; 160 * 160], 0.0)?;
        let mut row = Tracker::new(Config::default())?.process(base.clone())?;
        assert!(row.accepted);
        let roi = vec![0; protected.size * protected.size];
        let mut observed = Vec::new();
        for frame in 0..300 {
            row.frame = frame;
            row.timestamp_ms = frame as f64 * 10.0;
            let image = base.clone().with_timestamp(row.timestamp_ms)?;
            let views =
                protected.update_prepared(&image, Some(&row), roi.clone(), LearningPolicy::Full);
            legacy.update_prepared(&image, Some(&row), roi.clone(), LearningPolicy::Full);
            if frame == 299 {
                observed = views
                    .instant
                    .iter()
                    .enumerate()
                    .filter_map(|(index, value)| (*value != 128).then_some(index))
                    .collect();
            }
        }
        assert!(!observed.is_empty());
        assert!(observed.iter().all(|index| legacy.map[*index] == 0));
        let preserved = observed
            .iter()
            .filter(|index| protected.map[**index] > 128)
            .count();
        assert!(preserved * 100 >= observed.len() * 95);
        assert!(
            observed
                .iter()
                .all(|index| protected.learning_hits[*index] == 1900),
            "ROI collisions must not count as multiple teaching frames"
        );
        Ok(())
    }

    #[test]
    fn new_surface_and_legacy_updates_remain_fast_and_learning_history_resets() -> Result<()> {
        let mut protected = diagnostics(true)?;
        protected.begin_learning_frame();
        protected.learn_site(0, 255, true, true);
        assert_eq!(protected.map[0], 192);
        protected.learn_site(0, 255, true, true);
        assert_eq!(protected.map[0], 193);
        assert_eq!(protected.learning_hits[0], 1);
        protected.map[1] = 128;
        protected.learning_hits[1] = 10;
        protected.learn_site(1, 255, true, true);
        assert_eq!(
            protected.map[1], 192,
            "young evidence retains legacy consolidation"
        );
        protected.map[1] = 128;
        protected.learning_hits[1] = 1600;
        protected.begin_learning_frame();
        protected.learn_site(1, 255, true, true);
        assert_eq!(
            protected.map[1], 129,
            "mature neutral evidence is not unseen"
        );
        protected.map[2] = 0;
        protected.map[3] = 255;
        protected.learn_site(2, 255, true, true);
        protected.learn_site(3, 0, true, true);
        assert_eq!(&protected.map[2..4], &[0, 255]);
        protected.reset_history();
        assert_eq!(protected.learning_epoch, 0);
        assert!(protected.learning_hits.iter().all(|hits| *hits == 0));
        assert!(
            protected
                .last_learning_epoch
                .iter()
                .all(|epoch| *epoch == 0)
        );

        let mut legacy = diagnostics(false)?;
        legacy.map[0] = 128;
        legacy.learning_hits[0] = 1000;
        legacy.learn_site(0, 255, true, true);
        assert_eq!(legacy.map[0], 192);
        legacy.learn_site(0, 0, true, true);
        assert_eq!(legacy.map[0], 191);
        legacy.map[1] = 128;
        legacy.learning_hits[1] = 1000;
        legacy.learn_site(1, 0, true, true);
        assert_eq!(legacy.map[1], 64);
        Ok(())
    }

    #[test]
    fn new_surface_probation_never_rewrites_known_or_observed_neutral_cells() -> Result<()> {
        let mut protected = diagnostics(true)?;
        protected.begin_learning_frame();
        protected.teach_site(0, 255, LearningPolicy::NewSurface, true, true);
        assert_eq!(protected.map[0], 192);
        assert_eq!(protected.learning_hits[0], 1);
        protected.teach_site(0, 0, LearningPolicy::NewSurface, true, true);
        assert_eq!(
            protected.map[0], 192,
            "a newly seen cell is now protected too"
        );
        assert_eq!(protected.learning_hits[0], 1);

        // A nonneutral imported confidence cell has no age history but is
        // already known. Observed neutral cells retain their age evidence.
        protected.map[1] = 200;
        protected.map[2] = 128;
        protected.learning_hits[2] = 10;
        protected.map[3] = 128;
        protected.learning_hits[3] = 1600;
        protected.teach_site(1, 0, LearningPolicy::NewSurface, true, true);
        protected.teach_site(2, 255, LearningPolicy::NewSurface, true, true);
        protected.teach_site(3, 0, LearningPolicy::NewSurface, true, true);
        protected.teach_site(4, 255, LearningPolicy::Frozen, true, true);
        assert_eq!(&protected.map[1..5], &[200, 128, 128, 128]);
        assert_eq!(&protected.learning_hits[1..5], &[0, 10, 1600, 0]);
        protected.teach_site(0, 255, LearningPolicy::Full, true, true);
        assert_eq!(protected.map[0], 193, "ordinary consolidation resumes");
        Ok(())
    }

    #[test]
    fn unaccepted_row_cannot_add_new_surface_even_when_requested() -> Result<()> {
        let mut protected = diagnostics(true)?;
        let base = GrayFrame::new(160, 160, vec![0.5; 160 * 160], 0.0)?;
        let mut row = Tracker::new(Config::default())?.process(base.clone())?;
        row.accepted = false;
        let initial = protected.map.clone();
        let roi = vec![255; protected.size * protected.size];
        protected.update_prepared(&base, Some(&row), roi, LearningPolicy::NewSurface);
        assert_eq!(protected.map, initial);
        assert_eq!(protected.learning_epoch, 0);
        assert!(protected.learning_hits.iter().all(|hits| *hits == 0));
        Ok(())
    }

    #[test]
    fn guarded_first_hits_leave_uniform_surface_unknown_without_changing_legacy_learning()
    -> Result<()> {
        let mut guarded = diagnostics(true)?;
        let mut legacy = diagnostics(false)?;
        let image = GrayFrame::new(160, 160, vec![0.5; 160 * 160], 0.)?;
        let roi = vec![255; guarded.size * guarded.size];
        assert!(guarded.informative_sites(&roi).iter().all(|site| !*site));
        guarded.update_prepared(&image, None, roi.clone(), LearningPolicy::Guarded);
        legacy.update_prepared(&image, None, roi, LearningPolicy::Full);
        assert!(guarded.map.iter().all(|value| *value == 128));
        assert!(legacy.map.iter().any(|value| *value != 128));
        let roi: Vec<_> = guarded
            .points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                if p.is_none() {
                    128
                } else if (i % guarded.size + i / guarded.size) % 2 == 0 {
                    0
                } else {
                    255
                }
            })
            .collect();
        guarded.update_prepared(&image, None, roi, LearningPolicy::Guarded);
        assert!(guarded.map.iter().filter(|value| **value != 128).count() > 32);
        Ok(())
    }

    #[test]
    fn young_boundary_cells_retain_legacy_learning_before_later_maturity() -> Result<()> {
        let mut protected = diagnostics(true)?;
        let mut legacy = diagnostics(false)?;
        for age in [1, 10, 63, 64, 512, MATURE_TEACHING_FRAMES - 2] {
            protected.map[0] = 200;
            legacy.map[0] = 200;
            protected.learning_hits[0] = age;
            protected.begin_learning_frame();
            protected.learn_site(0, 0, true, true);
            legacy.learn_site(0, 0, true, true);
            assert_eq!(protected.map[0], legacy.map[0], "young boundary age {age}");
        }
        protected.map[0] = 200;
        protected.learning_hits[0] = MATURE_TEACHING_FRAMES;
        protected.begin_learning_frame();
        protected.learn_site(0, 0, true, true);
        assert_eq!(
            protected.map[0], 200,
            "mature contrary vote waits for its phase"
        );
        Ok(())
    }

    #[test]
    fn mature_counter_wrap_keeps_periodic_learning_alive() -> Result<()> {
        let mut protected = diagnostics(true)?;
        protected.map[0] = 200;
        protected.learning_hits[0] = u16::MAX - 1;
        for _ in 0..34 {
            protected.begin_learning_frame();
            protected.learn_site(0, 0, true, true);
        }
        assert_eq!(protected.map[0], 197);
        assert!(protected.learning_hits[0] >= u16::MAX - 15);
        Ok(())
    }
}

#[cfg(test)]
mod filter_tests {
    use super::*;
    #[test]
    fn median_network_exhausts_binary_threshold_cases() {
        for bits in 0..512 {
            let values = std::array::from_fn(|i| if bits & (1 << i) != 0 { 255 } else { 0 });
            let mut sorted = values;
            sorted.sort_unstable();
            assert_eq!(median9(values), sorted[4]);
        }
    }
    #[test]
    fn blocked_extrema_match_scalar_windows_with_masks_and_saturation() {
        for n in [1, 2, 3, 15, 60, 300] {
            let values: Vec<_> = (0..n)
                .map(|i| {
                    if i % 7 == 0 {
                        Extrema::EMPTY
                    } else {
                        let v = (i * 73 % 256) as u8;
                        Extrema {
                            min: v,
                            max: if v < 255 { v } else { 0 },
                        }
                    }
                })
                .collect();
            for rad in [0, 1, n / 2, n] {
                let mut prefix = vec![Extrema::EMPTY; n + 2 * rad];
                let mut suffix = prefix.clone();
                let mut result = vec![Extrema::EMPTY; n];
                extrema_windows(
                    n,
                    rad,
                    &mut prefix,
                    &mut suffix,
                    |i| values[i],
                    |i, v| result[i] = v,
                );
                for (i, v) in result.iter().enumerate() {
                    let expected = values[i.saturating_sub(rad)..=(i + rad).min(n - 1)]
                        .iter()
                        .copied()
                        .fold(Extrema::EMPTY, Extrema::join);
                    assert_eq!(*v, expected, "n={n} radius={rad} at {i}");
                }
            }
        }
    }
}
