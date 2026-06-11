#[cfg(feature = "image")]
pub(crate) mod image;
pub(crate) mod shapes;

use std::collections::BTreeMap;

use crate::document::Color;
use lopdf::{Dictionary, Object};

/// A pending draw operation (non-text).
pub(crate) enum DrawOp {
    Rect {
        rect: [f32; 4],
        color: Color,
        opacity: f32,
    },
    RectStroke {
        rect: [f32; 4],
        color: Color,
        line_width: f32,
        opacity: f32,
    },
    Line {
        from: [f32; 2],
        to: [f32; 2],
        color: Color,
        width: f32,
        opacity: f32,
    },
    Polygon {
        points: Vec<[f32; 2]>,
        color: Color,
        opacity: f32,
        filled: bool,
        stroke_width: f32,
    },
    Polyline {
        points: Vec<[f32; 2]>,
        color: Color,
        width: f32,
        opacity: f32,
    },
    Ellipse {
        rect: [f32; 4],
        color: Color,
        opacity: f32,
        filled: bool,
        stroke_width: f32,
    },
    Path {
        points: Vec<[f32; 2]>,
        closed: bool,
        color: Color,
        opacity: f32,
        filled: bool,
        stroke_width: f32,
    },
    #[cfg(feature = "image")]
    Image {
        bytes: Vec<u8>,
        rect: [f32; 4],
        opacity: f32,
    },
}

/// Maps opacity values → `/ExtGState` resource names, deduplicating equal opacities.
pub(crate) struct ExtGStateRegistry {
    /// key: (opacity * 1000).round() as u32, value: resource name e.g. "GS0"
    map: BTreeMap<u32, String>,
    next_idx: u32,
}

impl ExtGStateRegistry {
    pub fn new() -> Self {
        Self {
            map: BTreeMap::new(),
            next_idx: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Returns the resource name for `opacity`, registering it if not seen before.
    pub fn register(&mut self, opacity: f32) -> String {
        let key = (opacity.clamp(0.0, 1.0) * 1000.0).round() as u32;
        self.map
            .entry(key)
            .or_insert_with(|| {
                let name = format!("GS{}", self.next_idx);
                self.next_idx += 1;
                name
            })
            .clone()
    }

    /// Returns lopdf objects ready to insert into a page's `/Resources /ExtGState` dict.
    pub fn to_lopdf_dict(&self) -> Dictionary {
        let mut ext_g = Dictionary::new();
        for (key, name) in &self.map {
            let opacity = *key as f32 / 1000.0;
            let mut gs = Dictionary::new();
            gs.set("Type", Object::Name(b"ExtGState".to_vec()));
            gs.set("ca", Object::Real(opacity)); // non-stroking (fill) alpha
            gs.set("CA", Object::Real(opacity)); // stroking alpha
            ext_g.set(name.as_bytes(), Object::Dictionary(gs));
        }
        ext_g
    }
}
