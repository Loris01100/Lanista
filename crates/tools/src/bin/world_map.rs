//! `world_map --width 4096`: draws the world map's land, from the Natural Earth outlines
//! of `crates/tools/geo/land-50m.json` (public domain, as packaged in `TopoJSON` by
//! `world-atlas`), into `assets/ui/world_west.png` and `assets/ui/world_east.png`: an
//! equirectangular image of the whole world, longitudes -180 to 180 from left to right,
//! latitudes 90 to -90 from top to bottom, cut at Greenwich into two squares, each one
//! within the 2048 pixels a side every graphics card takes at the default width. The
//! sea is transparent: the game draws it and the parallels under the land, and centres
//! the world where it likes (`club_screen/world_map.rs`).

use std::path::Path;
use std::process::ExitCode;

use serde_json::Value;
use tiny_skia::{Color, FillRule, IntRect, Paint, PathBuilder, Pixmap, Stroke, Transform};
use tools::options;

/// The land's fill, the shore's line and the glow around it (`theme` colors of the game).
const LAND: [u8; 3] = [0x3A, 0x2F, 0x26];
const SHORE: [u8; 3] = [0x75, 0x59, 0x3F];
/// Opacity of the glow, out of 255, and its width over the shore's, at a width of 4096.
const GLOW_ALPHA: u8 = 40;
const GLOW_WIDTH: f32 = 6.0;
const SHORE_WIDTH: f32 = 1.2;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let [width] = options(std::env::args().skip(1), [("--width", 4096)])?;
    let width = u32::try_from(width).map_err(|_| "--width: too large".to_owned())?;
    if width < 2 {
        return Err("--width: at least 2".into());
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = root.join("geo/land-50m.json");
    let text =
        std::fs::read_to_string(&source).map_err(|e| format!("{}: {e}", source.display()))?;
    let topology: Value = serde_json::from_str(&text).map_err(|e| format!("not JSON: {e}"))?;
    let rings = land_rings(&topology)?;
    let pixmap = draw(&rings, width)?;
    let side = width / 2;
    for (half, left) in [("west", 0), ("east", side)] {
        let output = root.join(format!("../../assets/ui/world_{half}.png"));
        let square = IntRect::from_xywh(i32::try_from(left).unwrap_or(0), 0, side, side)
            .and_then(|r| pixmap.clone_rect(r))
            .ok_or("bad half")?;
        square
            .save_png(&output)
            .map_err(|e| format!("{}: {e}", output.display()))?;
        println!("{}: {side} × {side}", output.display());
    }
    println!("{} rings drawn", rings.len());
    Ok(())
}

/// A closed line of `(longitude, latitude)` points, in degrees.
type Ring = Vec<(f64, f64)>;

/// Every ring of the topology's `land` object, outer ones and holes alike: filled
/// even-odd, each polygon's holes come out.
fn land_rings(topology: &Value) -> Result<Vec<Vec<Ring>>, String> {
    let arcs = decoded_arcs(topology)?;
    let geometries = topology["objects"]["land"]["geometries"]
        .as_array()
        .ok_or("no land geometries")?;
    let mut polygons = Vec::new();
    for geometry in geometries {
        let shapes = &geometry["arcs"];
        let polygons_of: Vec<&Value> = match geometry["type"].as_str() {
            Some("Polygon") => vec![shapes],
            Some("MultiPolygon") => shapes
                .as_array()
                .ok_or("bad multipolygon")?
                .iter()
                .collect(),
            other => return Err(format!("unexpected geometry {other:?}")),
        };
        for polygon in polygons_of {
            let rings = polygon.as_array().ok_or("bad polygon")?;
            let rings: Result<Vec<Ring>, String> =
                rings.iter().map(|ring| stitched(ring, &arcs)).collect();
            polygons.push(rings?);
        }
    }
    Ok(polygons)
}

/// The topology's arcs in degrees: `TopoJSON` quantizes them and stores each point as
/// the difference from the one before.
fn decoded_arcs(topology: &Value) -> Result<Vec<Ring>, String> {
    let number = |v: &Value| v.as_f64().ok_or_else(|| "not a number".to_owned());
    let transform = &topology["transform"];
    let scale = (
        number(&transform["scale"][0])?,
        number(&transform["scale"][1])?,
    );
    let shift = (
        number(&transform["translate"][0])?,
        number(&transform["translate"][1])?,
    );
    let arcs = topology["arcs"].as_array().ok_or("no arcs")?;
    arcs.iter()
        .map(|arc| {
            let (mut x, mut y) = (0.0, 0.0);
            arc.as_array()
                .ok_or_else(|| "bad arc".to_owned())?
                .iter()
                .map(|point| {
                    x += number(&point[0])?;
                    y += number(&point[1])?;
                    Ok((x * scale.0 + shift.0, y * scale.1 + shift.1))
                })
                .collect()
        })
        .collect()
}

/// A ring from its arcs, each by index, `!i` for arc `i` backwards; consecutive arcs
/// share their end point, kept once.
fn stitched(ring: &Value, arcs: &[Ring]) -> Result<Ring, String> {
    let mut points = Ring::new();
    for index in ring.as_array().ok_or("bad ring")? {
        let index = index.as_i64().ok_or("bad arc index")?;
        let (i, backwards) = if index < 0 {
            (!index, true)
        } else {
            (index, false)
        };
        let arc = usize::try_from(i)
            .ok()
            .and_then(|i| arcs.get(i))
            .ok_or("arc index out of range")?;
        let start = usize::from(!points.is_empty());
        if backwards {
            points.extend(arc.iter().rev().skip(start));
        } else {
            points.extend(arc.iter().skip(start));
        }
    }
    Ok(points)
}

/// The land at `width` pixels wide: the glow first, then the fill, then the shore.
fn draw(polygons: &[Vec<Ring>], width: u32) -> Result<Pixmap, String> {
    let height = width / 2;
    let mut pixmap = Pixmap::new(width, height).ok_or("image too large")?;
    let scale = f64::from(width) / 360.0;
    #[expect(clippy::cast_possible_truncation, reason = "pixels within the image")]
    let at = |(lon, lat): (f64, f64)| {
        (
            ((lon + 180.0) * scale) as f32,
            ((90.0 - lat) * scale) as f32,
        )
    };
    let mut fill = PathBuilder::new();
    let mut shore = PathBuilder::new();
    for ring in polygons.iter().flatten() {
        let (ring, pole) = unwrapped(ring);
        // Drawn three times, a turn of the world apart: a ring across the 180th
        // meridian runs off one edge of the image and comes back in at the other.
        for turn in [-360.0, 0.0, 360.0] {
            let shifted = |(lon, lat): (f64, f64)| at((lon + turn, lat));
            let mut points = ring.iter().chain(&pole).map(|&p| shifted(p));
            let Some((x, y)) = points.next() else {
                continue;
            };
            fill.move_to(x, y);
            for (x, y) in points {
                fill.line_to(x, y);
            }
            fill.close();
            // No shore along the poles, where Natural Earth closes Antarctica.
            for pair in ring.windows(2) {
                if !polar(pair[0], pair[1]) {
                    let ((x0, y0), (x1, y1)) = (shifted(pair[0]), shifted(pair[1]));
                    shore.move_to(x0, y0);
                    shore.line_to(x1, y1);
                }
            }
        }
    }
    let fill = fill.finish().ok_or("no land")?;
    let shore = shore.finish().ok_or("no shore")?;
    #[expect(clippy::cast_precision_loss, reason = "an image width")]
    let zoom = width as f32 / 4096.0;
    let paint = |[r, g, b]: [u8; 3], alpha: u8| {
        let mut paint = Paint::default();
        paint.set_color(Color::from_rgba8(r, g, b, alpha));
        paint.anti_alias = true;
        paint
    };
    let line = |width: f32| Stroke {
        width: width * zoom,
        line_cap: tiny_skia::LineCap::Round,
        line_join: tiny_skia::LineJoin::Round,
        ..Stroke::default()
    };
    let none = Transform::identity();
    pixmap.stroke_path(
        &shore,
        &paint(SHORE, GLOW_ALPHA),
        &line(GLOW_WIDTH),
        none,
        None,
    );
    pixmap.fill_path(&fill, &paint(LAND, 255), FillRule::EvenOdd, none, None);
    pixmap.stroke_path(&shore, &paint(SHORE, 255), &line(SHORE_WIDTH), none, None);
    Ok(pixmap)
}

/// `ring` with no jump of a turn of the world between two points: each longitude
/// within half a turn of the one before. A ring that goes all the way round the world
/// (Antarctica) then ends a turn from where it started: it is closed along its pole,
/// the points returned beside it.
fn unwrapped(ring: &Ring) -> (Ring, Vec<(f64, f64)>) {
    let mut points = Ring::with_capacity(ring.len());
    for &(lon, lat) in ring {
        let lon = match points.last() {
            Some(&(before, _)) => lon + 360.0 * ((before - lon) / 360.0).round(),
            None => lon,
        };
        points.push((lon, lat));
    }
    let pole = match (points.first(), points.last()) {
        (Some(&(first, _)), Some(&(last, _))) if (last - first).abs() > 180.0 => {
            let side = if points.iter().map(|p| p.1).sum::<f64>() < 0.0 {
                -90.0
            } else {
                90.0
            };
            vec![(last, side), (first, side)]
        }
        _ => Vec::new(),
    };
    (points, pole)
}

/// Whether the segment from `a` to `b` runs along a pole rather than a coast.
fn polar(a: (f64, f64), b: (f64, f64)) -> bool {
    a.1.abs() > 84.0 && b.1.abs() > 84.0
}
