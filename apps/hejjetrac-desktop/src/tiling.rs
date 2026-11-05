//! Fit each native window to its image, accounting for its actual decorations.
use eframe::egui::{self, Pos2, Rect, Vec2};

#[derive(Clone, Copy, Debug)]
pub struct Tile {
    pub position: Pos2,
    pub inner: Vec2,
}
/// EWMH work area excludes docks/panels; RandR identifies the current monitor.
/// All protocol I/O is safe Rust and is used only for an explicit arrangement.
#[cfg(target_os = "linux")]
pub fn native_work_area(ctx: &egui::Context) -> Option<Rect> {
    use x11rb::{
        connection::Connection,
        protocol::{
            randr::ConnectionExt as _,
            xproto::{AtomEnum, ConnectionExt as _},
        },
    };
    let (connection, screen) = x11rb::connect(None).ok()?;
    let root = connection.setup().roots.get(screen)?.root;
    let atom = |name: &[u8]| {
        connection
            .intern_atom(false, name)
            .ok()?
            .reply()
            .ok()
            .map(|r| r.atom)
    };
    let desktop = atom(b"_NET_CURRENT_DESKTOP").and_then(|atom| {
        let reply = connection
            .get_property(false, root, atom, AtomEnum::CARDINAL, 0, 1)
            .ok()?
            .reply()
            .ok()?;
        reply.value32()?.next().map(|value| value as usize)
    });
    let reply = connection
        .get_property(
            false,
            root,
            atom(b"_NET_WORKAREA")?,
            AtomEnum::CARDINAL,
            0,
            256,
        )
        .ok()?
        .reply()
        .ok()?;
    let areas: Vec<_> = reply.value32()?.collect();
    let work = work_area_values(&areas, desktop)?;
    let scale = ctx.pixels_per_point();
    let centre = ctx.input(|i| i.viewport().outer_rect.map(|r| r.center()))? * scale;
    let monitors = connection
        .randr_query_version(1, 5)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .and_then(|_| connection.randr_get_monitors(root, true).ok()?.reply().ok());
    let monitor = monitors
        .as_ref()
        .and_then(|reply| {
            reply
                .monitors
                .iter()
                .map(|m| {
                    Rect::from_min_size(
                        egui::pos2(m.x as f32, m.y as f32),
                        egui::vec2(m.width as f32, m.height as f32),
                    )
                })
                .find(|rect| rect.contains(centre))
        })
        .unwrap_or(work);
    let usable = work.intersect(monitor).shrink(4.0);
    (usable.is_positive() && scale.is_finite() && scale > 0.0)
        .then(|| Rect::from_min_max(usable.min / scale, usable.max / scale))
}
#[cfg(any(target_os = "linux", test))]
fn work_area_values(areas: &[u32], desktop: Option<usize>) -> Option<Rect> {
    let (areas, remainder) = areas.as_chunks::<4>();
    if !remainder.is_empty() {
        return None;
    }
    let rect = |values: &[u32; 4]| {
        Rect::from_min_size(
            egui::pos2(values[0] as i32 as f32, values[1] as i32 as f32),
            egui::vec2(values[2] as f32, values[3] as f32),
        )
    };
    if let Some(desktop) = desktop {
        return areas.get(desktop).map(rect);
    }
    // XWayland can omit CURRENT_DESKTOP while exporting each work area.
    // Use their intersection so a missing index cannot put a window over a dock.
    let (first, remaining) = areas.split_first()?;
    let mut work = rect(first);
    for values in remaining {
        work = work.intersect(rect(values));
    }
    work.is_positive().then_some(work)
}
#[cfg(not(target_os = "linux"))]
pub fn native_work_area(_ctx: &egui::Context) -> Option<Rect> {
    None
}
pub fn chrome(size: Vec2) -> f32 {
    let toolbar = if size.x >= 112.0 && size.y >= 120.0 {
        40.0
    } else {
        0.0
    };
    let status = if size.x >= 112.0 && size.y >= 160.0 {
        22.0
    } else {
        0.0
    };
    toolbar + status
}
fn fit(available: Vec2, aspect: f32) -> Vec2 {
    let mut best = Vec2::ZERO;
    for reserved in [62.0, 40.0, 0.0] {
        let height = (available.y - reserved).floor().max(0.0);
        let width = available.x.floor().min((height * aspect).floor());
        let size = egui::vec2(width, width / aspect + reserved);
        if chrome(size) == reserved
            && size.x <= available.x
            && size.y <= available.y
            && size.x > best.x
        {
            best = size;
        }
    }
    best
}
#[cfg(test)]
pub fn plan(work: Rect, aspects: &[f32], decorations: Vec2, columns: Option<usize>) -> Vec<Tile> {
    plan_layout(work, aspects, decorations, columns, None)
}
pub fn plan_layout(
    work: Rect,
    aspects: &[f32],
    decorations: Vec2,
    columns: Option<usize>,
    rows: Option<usize>,
) -> Vec<Tile> {
    if aspects.is_empty()
        || aspects.len() > 16
        || !work.is_finite()
        || work.width() < 96.0
        || work.height() < 96.0
    {
        return vec![];
    }
    let aspects: Vec<_> = aspects
        .iter()
        .map(|a| {
            if a.is_finite() {
                a.clamp(0.25, 4.0)
            } else {
                4.0 / 3.0
            }
        })
        .collect();
    let decorations = decorations.max(Vec2::ZERO).min(egui::vec2(64.0, 96.0));
    let gap = 4.0;
    let columns = Some(
        columns
            .unwrap_or_else(|| {
                rows.map(|r| aspects.len().div_ceil(r.max(1)))
                    .unwrap_or_else(|| square_columns(aspects.len()))
            })
            .clamp(1, 16),
    );
    let mut best = (0.0, vec![]);
    for cols in 1..=16 {
        if columns.is_some_and(|desired| desired != cols) {
            continue;
        }
        let rows = rows.unwrap_or_else(|| aspects.len().div_ceil(cols));
        if rows == 0 || rows > 16 || rows * cols < aspects.len() {
            continue;
        }
        let cell = egui::vec2(
            (work.width() - gap * (cols - 1) as f32) / cols as f32,
            (work.height() - gap * (rows - 1) as f32) / rows as f32,
        );
        let available = cell - decorations;
        if available.x < 96.0 || available.y < 96.0 {
            continue;
        }
        let mut tiles = vec![];
        let mut area = 0.0;

        for (row_index, row) in aspects.chunks(cols).enumerate() {
            let y = work.top() + row_index as f32 * (cell.y + gap);
            let sizes: Vec<_> = row.iter().map(|a| fit(available, *a)).collect();
            if sizes.iter().any(|s| s.x < 96.0 || s.y < 96.0) {
                break;
            }

            for (column_index, inner) in sizes.into_iter().enumerate() {
                let x = work.left() + column_index as f32 * (cell.x + gap);
                tiles.push(Tile {
                    position: egui::pos2(x, y),
                    inner,
                });
                area += inner.x * (inner.y - chrome(inner));
            }
        }
        if tiles.len() == aspects.len() && area > best.0 {
            best = (area, tiles);
        }
    }
    best.1
}

pub fn square_columns(count: usize) -> usize {
    (count.clamp(1, 16) as f32).sqrt().ceil() as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_xwayland_desktop_index_keeps_dock_reservation() {
        let values = [70, 27, 1610, 1023, 70, 27, 1610, 1023];
        assert_eq!(
            work_area_values(&values, None),
            work_area_values(&values, Some(1))
        );
        assert_eq!(
            work_area_values(&values, None).map(|r| r.left()),
            Some(70.0)
        );
        assert!(work_area_values(&values, Some(2)).is_none());
    }
    #[test]
    fn four_and_sixteen_fit_without_overlap_or_pillarboxing() {
        let work = Rect::from_min_size(egui::pos2(4.0, 32.0), egui::vec2(1912.0, 1036.0));
        for count in [4, 16] {
            for aspect in [1.0, 4.0 / 3.0] {
                let tiles = plan(work, &vec![aspect; count], egui::vec2(0.0, 37.0), None);
                assert_eq!(tiles.len(), count);
                for (i, tile) in tiles.iter().enumerate() {
                    let outer =
                        Rect::from_min_size(tile.position, tile.inner + egui::vec2(0.0, 37.0));
                    assert!(work.contains_rect(outer));
                    assert!(
                        (tile.inner.x / (tile.inner.y - chrome(tile.inner)) - aspect).abs() < 0.001
                    );
                    for other in &tiles[..i] {
                        assert!(!outer.intersects(Rect::from_min_size(
                            other.position,
                            other.inner + egui::vec2(0.0, 37.0)
                        )));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    #[test]
    fn square_default_and_reserved_rows_keep_exact_cells() {
        let work = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1920., 1080.));
        let tiles = plan_layout(work, &[4. / 3.; 16], Vec2::ZERO, None, None);
        assert_eq!(tiles.len(), 16);
        assert_eq!(tiles[0].position.y, tiles[3].position.y);
        assert!(tiles[4].position.y > tiles[0].position.y);
        assert_eq!(tiles[12].position.x, tiles[0].position.x);
        let reserved = plan_layout(work, &[4. / 3.; 8], Vec2::ZERO, Some(4), Some(4));
        assert_eq!(reserved.len(), 8);
        assert_eq!(reserved[4].position, tiles[4].position);
        assert!(plan_layout(work, &[1.; 16], Vec2::ZERO, Some(3), Some(3)).is_empty());
        assert_eq!(
            (1..=16).map(square_columns).collect::<Vec<_>>(),
            vec![1, 2, 2, 2, 3, 3, 3, 3, 3, 4, 4, 4, 4, 4, 4, 4]
        );
    }
}

/// Equal dashboard cells; the preview fits its image inside each cell.
pub fn grid(work: Rect, count: usize, columns: Option<usize>, rows: Option<usize>) -> Vec<Tile> {
    if count == 0 || count > 16 || !work.is_finite() {
        return vec![];
    }
    let cols = columns.unwrap_or_else(|| {
        rows.map(|r| count.div_ceil(r.max(1)))
            .unwrap_or_else(|| square_columns(count))
    });
    if !(1..=16).contains(&cols) {
        return vec![];
    }
    let rows = rows.unwrap_or_else(|| count.div_ceil(cols));
    if !(1..=16).contains(&rows) || cols * rows < count {
        return vec![];
    }
    let size = egui::vec2(
        (work.width() - 4. * (cols - 1) as f32) / cols as f32,
        (work.height() - 4. * (rows - 1) as f32) / rows as f32,
    );
    if size.x < 96. || size.y < 96. {
        return vec![];
    }
    (0..count)
        .map(|i| Tile {
            position: work.min
                + egui::vec2(
                    (i % cols) as f32 * (size.x + 4.),
                    (i / cols) as f32 * (size.y + 4.),
                ),
            inner: size,
        })
        .collect()
}
