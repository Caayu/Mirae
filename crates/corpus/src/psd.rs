//! Minimal writer for layered test documents in the public PSD format
//! (Adobe Photoshop File Formats Specification, RGB, raw channels, 8 or
//! 16 bits per channel).
//!
//! Parity probes only: the reference application opens these documents we
//! wrote and exports PNGs through its normal UI, which are then measured
//! (docs/PARITY.md, black-box rules). Tela itself does not read or write
//! PSD.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// A channel sample the format can store: `u8` (8-bit documents) or `u16`
/// (16-bit documents, big-endian).
pub trait PsdSample: Copy {
    /// Bits per channel written in the header.
    const DEPTH: u16;
    /// Appends the sample, big-endian.
    fn push(self, out: &mut Vec<u8>);
}

impl PsdSample for u8 {
    const DEPTH: u16 = 8;
    fn push(self, out: &mut Vec<u8>) {
        out.push(self);
    }
}

impl PsdSample for u16 {
    const DEPTH: u16 = 16;
    fn push(self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_be_bytes());
    }
}

/// A layer mask: values over `rect` (maximum reveals, 0 hides, as the
/// format defines), `default` outside it.
#[derive(Clone, Debug)]
pub struct PsdMask<V = u8> {
    /// `(left, top, width, height)` in document pixels.
    pub rect: (u32, u32, u32, u32),
    /// Row-major, `width × height` values.
    pub values: Vec<V>,
    /// Value outside `rect` (0 or 255).
    pub default: u8,
    /// Stored but not applied ("layer mask disabled" flag).
    pub disabled: bool,
}

/// One raster layer; `V` is the channel sample (`u8` or `u16`).
#[derive(Clone, Debug)]
pub struct PsdLayer<V = u8> {
    pub name: String,
    /// Blend-mode key of the specification (`b"norm"`, `b"mul "`, …).
    pub mode: [u8; 4],
    /// 0–255.
    pub opacity: u8,
    /// Clipped to the layer below ("clipping: non-base").
    pub clipped: bool,
    pub hidden: bool,
    /// `(left, top, width, height)` in document pixels.
    pub rect: (u32, u32, u32, u32),
    /// Straight-alpha RGBA, row-major over `rect`.
    pub rgba: Vec<[V; 4]>,
    pub mask: Option<PsdMask<V>>,
}

fn be16(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_be_bytes());
}

fn be32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_be_bytes());
}

fn rect_edges(v: &mut Vec<u8>, (left, top, w, h): (u32, u32, u32, u32)) {
    be32(v, top);
    be32(v, left);
    be32(v, top + h);
    be32(v, left + w);
}

/// Writes `layers` (bottom first) as a `width × height` 8-bit PSD. The
/// merged image is plain white; readers recompose from the layers.
pub fn write_psd(path: &Path, width: u32, height: u32, layers: &[PsdLayer]) -> std::io::Result<()> {
    write_layers(path, width, height, layers)
}

/// Like [`write_psd`], with 16 bits per channel (masks included).
pub fn write_psd16(
    path: &Path,
    width: u32,
    height: u32,
    layers: &[PsdLayer<u16>],
) -> std::io::Result<()> {
    write_layers(path, width, height, layers)
}

fn write_layers<V: PsdSample>(
    path: &Path,
    width: u32,
    height: u32,
    layers: &[PsdLayer<V>],
) -> std::io::Result<()> {
    let bytes = u32::from(V::DEPTH / 8);
    let mut info = Vec::new();
    be16(&mut info, layers.len() as u16);
    for l in layers {
        let (_, _, w, h) = l.rect;
        assert_eq!(l.rgba.len(), (w * h) as usize, "{}: pixel count", l.name);
        rect_edges(&mut info, l.rect);
        // Channels: alpha (-1), R, G, B, then the user mask (-2).
        let mut channels: Vec<(i16, u32)> = vec![(-1, w * h), (0, w * h), (1, w * h), (2, w * h)];
        if let Some(m) = &l.mask {
            let (_, _, mw, mh) = m.rect;
            assert_eq!(m.values.len(), (mw * mh) as usize, "{}: mask size", l.name);
            channels.push((-2, mw * mh));
        }
        be16(&mut info, channels.len() as u16);
        for (id, n) in &channels {
            be16(&mut info, *id as u16);
            be32(&mut info, 2 + n * bytes);
        }
        info.extend_from_slice(b"8BIM");
        info.extend_from_slice(&l.mode);
        info.push(l.opacity);
        info.push(u8::from(l.clipped));
        info.push(if l.hidden { 2 } else { 0 });
        info.push(0); // filler
        let mut name = vec![l.name.len() as u8];
        name.extend_from_slice(l.name.as_bytes());
        while name.len() % 4 != 0 {
            name.push(0);
        }
        let mut mask = Vec::new();
        if let Some(m) = &l.mask {
            rect_edges(&mut mask, m.rect);
            mask.push(m.default);
            mask.push(if m.disabled { 2 } else { 0 });
            mask.extend_from_slice(&[0, 0]); // padding to 20 bytes
        }
        be32(&mut info, (4 + mask.len() + 4 + name.len()) as u32);
        be32(&mut info, mask.len() as u32);
        info.extend_from_slice(&mask);
        be32(&mut info, 0); // no blending ranges
        info.extend_from_slice(&name);
    }
    for l in layers {
        for c in [3, 0, 1, 2] {
            be16(&mut info, 0); // raw
            for p in &l.rgba {
                p[c].push(&mut info);
            }
        }
        if let Some(m) = &l.mask {
            be16(&mut info, 0);
            for v in &m.values {
                v.push(&mut info);
            }
        }
    }
    // 16-bit documents keep their layers in an "Lr16" block of the
    // additional layer information; the plain layer info stays empty.
    let section = if V::DEPTH == 16 {
        while info.len() % 4 != 0 {
            info.push(0);
        }
        let mut s = Vec::new();
        be32(&mut s, 0); // layer info
        be32(&mut s, 0); // global layer mask info
        s.extend_from_slice(b"8BIMLr16");
        be32(&mut s, info.len() as u32);
        s.extend_from_slice(&info);
        s
    } else {
        if info.len() % 2 != 0 {
            info.push(0);
        }
        let mut s = Vec::new();
        be32(&mut s, info.len() as u32);
        s.extend_from_slice(&info);
        be32(&mut s, 0); // global layer mask info
        s
    };

    let mut f = BufWriter::new(File::create(path)?);
    let mut head = Vec::new();
    head.extend_from_slice(b"8BPS");
    be16(&mut head, 1);
    head.extend_from_slice(&[0; 6]);
    be16(&mut head, 3); // composite channels
    be32(&mut head, height);
    be32(&mut head, width);
    be16(&mut head, V::DEPTH);
    be16(&mut head, 3); // RGB
    be32(&mut head, 0); // color mode data
    be32(&mut head, 0); // image resources
    be32(&mut head, section.len() as u32); // layer and mask info
    f.write_all(&head)?;
    f.write_all(&section)?;
    let mut merged = Vec::new();
    be16(&mut merged, 0);
    merged.resize(2 + (3 * width * height * bytes) as usize, 255);
    f.write_all(&merged)?;
    f.flush()
}
