//! `manifest.json`: the persisted model of a document, separate from the
//! domain model (`tela_document`). Writing serializes plain DTOs; reading
//! walks the JSON value field by field, so every error names the field
//! and every field this version does not know becomes a warning.
//!
//! Numbers: ids and sizes are JSON integers; opacity and paper channels
//! are the exact `f32` values written as JSON numbers through `f64`
//! (exact), so reading them as `f64` and narrowing to `f32` gives the same
//! bits back even if a JSON parser were off by a few `f64` ulps.

use std::collections::HashSet;

use serde::Serialize;
use serde_json::{Map, Value};
use tela_canvas::TileCoord;
use tela_color::{PixelDepth, Rgba};
use tela_document::{BlendMode, Document, LayerPart, MAX_TILE_COORD, PartsError};

use crate::format::{FORMAT_NAME, FORMAT_VERSION, MASK_TILE_BYTES, pixel_tile_bytes};
use crate::limits::check;
use crate::{IoError, Limits, Warning};

/// One layer as the manifest describes it.
#[derive(Debug)]
pub(crate) struct LayerEntry {
    pub id: u32,
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub blend_mode: BlendMode,
    pub locked: bool,
    pub alpha_locked: bool,
    pub clipped: bool,
    /// Stored pixel tiles, in file order.
    pub pixels: Vec<TileCoord>,
    /// `(enabled, stored tiles)`.
    pub mask: Option<(bool, Vec<TileCoord>)>,
}

/// The manifest, parsed and checked against the limits (not yet against
/// the document's invariants: `Document::from_parts` does that).
#[derive(Debug)]
pub(crate) struct Manifest {
    pub width: u32,
    pub height: u32,
    pub depth: PixelDepth,
    pub paper: Rgba,
    pub next_layer_id: Option<u32>,
    pub active_layer: u32,
    pub active_part: LayerPart,
    pub layers: Vec<LayerEntry>,
}

pub(crate) fn depth_name(depth: PixelDepth) -> &'static str {
    match depth {
        PixelDepth::Rgba8 => "rgba8",
        PixelDepth::Rgba16 => "rgba16",
    }
}

fn part_name(part: LayerPart) -> &'static str {
    match part {
        LayerPart::Pixels => "pixels",
        LayerPart::Mask => "mask",
    }
}

// ---------------------------------------------------------------- writing

#[derive(Serialize)]
struct ManifestOut<'a> {
    format: &'static str,
    version: u32,
    writer: &'static str,
    canvas: CanvasOut,
    depth: &'static str,
    paper: [f64; 4],
    next_layer_id: Option<u32>,
    active_layer: u32,
    active_part: &'static str,
    layers: Vec<LayerOut<'a>>,
}

#[derive(Serialize)]
struct CanvasOut {
    width: u32,
    height: u32,
}

#[derive(Serialize)]
struct LayerOut<'a> {
    id: u32,
    name: &'a str,
    visible: bool,
    opacity: f64,
    blend: &'static str,
    locked: bool,
    alpha_locked: bool,
    clipped: bool,
    pixels: &'a [[i32; 2]],
    mask: Option<MaskOut<'a>>,
}

#[derive(Serialize)]
struct MaskOut<'a> {
    enabled: bool,
    tiles: &'a [[i32; 2]],
}

/// Tile coordinates in file order: rows top to bottom, left to right.
pub(crate) fn sorted(coords: impl Iterator<Item = TileCoord>) -> Vec<TileCoord> {
    let mut v: Vec<TileCoord> = coords.collect();
    v.sort_unstable_by_key(|c| (c.y, c.x));
    v
}

fn pairs(coords: &[TileCoord]) -> Vec<[i32; 2]> {
    coords.iter().map(|c| [c.x, c.y]).collect()
}

/// The manifest of `doc` as JSON bytes, with the tile lists in the order
/// the tile entries are written.
pub(crate) fn write(doc: &Document) -> Vec<u8> {
    type Lists = (Vec<[i32; 2]>, Option<Vec<[i32; 2]>>);
    let tiles: Vec<Lists> = doc
        .layers()
        .map(|l| {
            let pixels = pairs(&sorted(l.pixels().tile_coords()));
            let mask = l.mask().map(|m| pairs(&sorted(m.tile_coords())));
            (pixels, mask)
        })
        .collect();
    let layers = doc
        .layers()
        .zip(&tiles)
        .map(|(l, (pixels, mask))| LayerOut {
            id: l.id().get(),
            name: l.name(),
            visible: l.visible(),
            opacity: f64::from(l.opacity()),
            blend: l.blend_mode().name(),
            locked: l.locked(),
            alpha_locked: l.alpha_locked(),
            clipped: l.clipped(),
            pixels,
            mask: l.mask().zip(mask.as_deref()).map(|(m, tiles)| MaskOut {
                enabled: m.enabled(),
                tiles,
            }),
        })
        .collect();
    let p = doc.background();
    let out = ManifestOut {
        format: FORMAT_NAME,
        version: FORMAT_VERSION,
        writer: concat!("Mirae ", env!("CARGO_PKG_VERSION")),
        canvas: CanvasOut {
            width: doc.width(),
            height: doc.height(),
        },
        depth: depth_name(doc.depth()),
        paper: [p.r, p.g, p.b, p.a].map(f64::from),
        next_layer_id: doc.next_layer_id(),
        active_layer: doc.active_layer().get(),
        active_part: part_name(doc.active_part()),
        layers,
    };
    serde_json::to_vec(&out).expect("plain data with string keys always serializes")
}

// ---------------------------------------------------------------- reading

fn invalid(msg: String) -> IoError {
    IoError::InvalidManifest(msg)
}

/// The fields of one JSON object, taken one by one; the rest are reported
/// as unknown when the object is finished.
struct Fields<'a> {
    path: String,
    map: &'a Map<String, Value>,
    taken: HashSet<&'static str>,
}

impl<'a> Fields<'a> {
    fn of(value: &'a Value, path: String) -> Result<Self, IoError> {
        match value {
            Value::Object(map) => Ok(Self {
                path,
                map,
                taken: HashSet::new(),
            }),
            _ => Err(invalid(format!("{} must be an object", show(&path)))),
        }
    }

    fn at(&self, key: &str) -> String {
        if self.path.is_empty() {
            key.to_owned()
        } else {
            format!("{}.{key}", self.path)
        }
    }

    fn opt(&mut self, key: &'static str) -> Option<&'a Value> {
        self.taken.insert(key);
        self.map.get(key).filter(|v| !v.is_null())
    }

    fn req(&mut self, key: &'static str) -> Result<&'a Value, IoError> {
        self.opt(key)
            .ok_or_else(|| invalid(format!("`{}` is missing", self.at(key))))
    }

    fn wrong(&self, key: &str, what: &str) -> IoError {
        invalid(format!("`{}` must be {what}", self.at(key)))
    }

    fn u32(&mut self, key: &'static str) -> Result<u32, IoError> {
        let v = self.req(key)?;
        v.as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| self.wrong(key, "an integer from 0 to 4294967295"))
    }

    /// A field that must be present and is either `null` or a `u32`.
    fn nullable_u32(&mut self, key: &'static str) -> Result<Option<u32>, IoError> {
        if !self.map.contains_key(key) {
            return Err(invalid(format!("`{}` is missing", self.at(key))));
        }
        match self.opt(key) {
            None => Ok(None),
            Some(v) => v
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .map(Some)
                .ok_or_else(|| self.wrong(key, "null or an integer from 0 to 4294967295")),
        }
    }

    fn bool(&mut self, key: &'static str) -> Result<bool, IoError> {
        let v = self.req(key)?;
        v.as_bool().ok_or_else(|| self.wrong(key, "true or false"))
    }

    fn str(&mut self, key: &'static str) -> Result<&'a str, IoError> {
        let v = self.req(key)?;
        v.as_str().ok_or_else(|| self.wrong(key, "a string"))
    }

    /// An `f32` written as a JSON number (see the module docs).
    fn f32(&mut self, key: &'static str) -> Result<f32, IoError> {
        let v = self.req(key)?;
        v.as_f64()
            .map(|x| x as f32)
            .ok_or_else(|| self.wrong(key, "a number"))
    }

    fn array(&mut self, key: &'static str) -> Result<&'a Vec<Value>, IoError> {
        let v = self.req(key)?;
        v.as_array().ok_or_else(|| self.wrong(key, "an array"))
    }

    fn finish(self, warnings: &mut Vec<Warning>) {
        let mut unknown: Vec<&String> = self
            .map
            .keys()
            .filter(|k| !self.taken.contains(k.as_str()))
            .collect();
        unknown.sort();
        for k in unknown {
            warnings.push(Warning::UnknownField(self.at(k)));
        }
    }
}

fn show(path: &str) -> String {
    if path.is_empty() {
        "the manifest".to_owned()
    } else {
        format!("`{path}`")
    }
}

/// Running totals checked against the limits while tile lists are read.
struct Budget<'l> {
    limits: &'l Limits,
    tiles: u64,
    bytes: u64,
}

impl Budget<'_> {
    fn add(&mut self, tiles: usize, tile_bytes: usize) -> Result<(), IoError> {
        self.tiles += tiles as u64;
        self.bytes += (tiles * tile_bytes) as u64;
        check("tiles", self.tiles, self.limits.max_tiles as u64)?;
        check(
            "bytes of pixel data",
            self.bytes,
            self.limits.max_pixel_bytes,
        )
    }
}

/// A list of `[x, y]` tile coordinates, in range and without duplicates.
fn tile_list(list: &[Value], path: &str) -> Result<Vec<TileCoord>, IoError> {
    let range = i64::from(-MAX_TILE_COORD - 1)..=i64::from(MAX_TILE_COORD);
    let mut seen = HashSet::with_capacity(list.len());
    let mut out = Vec::with_capacity(list.len());
    for (i, v) in list.iter().enumerate() {
        let xy = v
            .as_array()
            .filter(|a| a.len() == 2)
            .and_then(|a| Some((a[0].as_i64()?, a[1].as_i64()?)))
            .filter(|(x, y)| range.contains(x) && range.contains(y))
            .ok_or_else(|| {
                invalid(format!(
                    "`{path}[{i}]` must be [x, y] with tile coordinates within ±{MAX_TILE_COORD}"
                ))
            })?;
        let c = TileCoord {
            x: xy.0 as i32,
            y: xy.1 as i32,
        };
        if !seen.insert(c) {
            return Err(invalid(format!(
                "`{path}` lists tile ({}, {}) twice",
                c.x, c.y
            )));
        }
        out.push(c);
    }
    Ok(out)
}

/// Parses and checks the manifest. The version is checked before
/// anything else, so a newer document fails as such and not as a
/// confusing missing field.
pub(crate) fn read(
    bytes: &[u8],
    limits: &Limits,
    warnings: &mut Vec<Warning>,
) -> Result<Manifest, IoError> {
    let root: Value =
        serde_json::from_slice(bytes).map_err(|e| invalid(format!("not valid JSON ({e})")))?;
    let mut top = Fields::of(&root, String::new())?;
    if top.str("format")? != FORMAT_NAME {
        return Err(invalid(format!("`format` must be \"{FORMAT_NAME}\"")));
    }
    let version = top
        .req("version")?
        .as_u64()
        .ok_or_else(|| top.wrong("version", "a positive integer"))?;
    if version == 0 {
        return Err(invalid("version 0 does not exist".to_owned()));
    }
    if version > u64::from(FORMAT_VERSION) {
        return Err(IoError::UnsupportedVersion {
            found: version,
            supported: FORMAT_VERSION,
        });
    }
    // Informational only.
    let _ = top.opt("writer");

    let mut canvas = Fields::of(top.req("canvas")?, "canvas".to_owned())?;
    let width = canvas.u32("width")?;
    let height = canvas.u32("height")?;
    canvas.finish(warnings);
    let side = u64::from(width.max(height));
    check(
        "pixels of canvas side",
        side,
        u64::from(limits.max_canvas_side),
    )?;

    let depth = match top.str("depth")? {
        "rgba8" => PixelDepth::Rgba8,
        "rgba16" => PixelDepth::Rgba16,
        other => return Err(IoError::Unsupported(format!("pixel depth \"{other}\""))),
    };
    let paper = match top.array("paper")?.as_slice() {
        [r, g, b, a] => {
            let c = [r, g, b, a].map(|v| v.as_f64().map(|x| x as f32));
            match c {
                [Some(r), Some(g), Some(b), Some(a)] => Rgba::new(r, g, b, a),
                _ => return Err(top.wrong("paper", "four numbers")),
            }
        }
        _ => return Err(top.wrong("paper", "four numbers")),
    };
    let next_layer_id = top.nullable_u32("next_layer_id")?;
    let active_layer = top.u32("active_layer")?;
    let active_part = match top.str("active_part")? {
        "pixels" => LayerPart::Pixels,
        "mask" => LayerPart::Mask,
        _ => return Err(top.wrong("active_part", "\"pixels\" or \"mask\"")),
    };

    let list = top.array("layers")?;
    check("layers", list.len() as u64, limits.max_layers as u64)?;
    let mut budget = Budget {
        limits,
        tiles: 0,
        bytes: 0,
    };
    let mut ids = HashSet::with_capacity(list.len());
    let mut layers = Vec::with_capacity(list.len());
    for (i, v) in list.iter().enumerate() {
        let path = format!("layers[{i}]");
        let mut l = Fields::of(v, path.clone())?;
        let id = l.u32("id")?;
        // Tile entries are named by id: a duplicate would make two layers
        // share entries, so report it as what it is before reading them.
        if id == 0 {
            return Err(IoError::InvalidDocument(PartsError::ZeroId));
        }
        if !ids.insert(id) {
            return Err(IoError::InvalidDocument(PartsError::DuplicateId(id)));
        }
        let name = l.str("name")?;
        check(
            "bytes of layer name",
            name.len() as u64,
            limits.max_name_bytes as u64,
        )?;
        let blend_name = l.str("blend")?;
        let blend_mode = BlendMode::from_name(blend_name)
            .ok_or_else(|| IoError::Unsupported(format!("blend mode \"{blend_name}\"")))?;
        let pixels = tile_list(l.array("pixels")?, &format!("{path}.pixels"))?;
        budget.add(pixels.len(), pixel_tile_bytes(depth))?;
        let mask = match l.opt("mask") {
            None => None,
            Some(m) => {
                let mut m = Fields::of(m, format!("{path}.mask"))?;
                let enabled = m.bool("enabled")?;
                let tiles = tile_list(m.array("tiles")?, &format!("{path}.mask.tiles"))?;
                budget.add(tiles.len(), MASK_TILE_BYTES)?;
                m.finish(warnings);
                Some((enabled, tiles))
            }
        };
        let entry = LayerEntry {
            id,
            name: name.to_owned(),
            visible: l.bool("visible")?,
            opacity: l.f32("opacity")?,
            blend_mode,
            locked: l.bool("locked")?,
            alpha_locked: l.bool("alpha_locked")?,
            clipped: l.bool("clipped")?,
            pixels,
            mask,
        };
        l.finish(warnings);
        layers.push(entry);
    }
    top.finish(warnings);
    Ok(Manifest {
        width,
        height,
        depth,
        paper,
        next_layer_id,
        active_layer,
        active_part,
        layers,
    })
}
