//! The reference's 3×5 ASCII atlas, area-sampled into sharp device-pixel runs.
//! This is a font atlas, not a byte-code mask or a screenshot of the document.
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};
const FONT: [[u8; 5]; 95] = [
    [0, 0, 0, 0, 0],
    [2, 2, 2, 0, 2],
    [5, 5, 0, 0, 0],
    [5, 7, 5, 7, 5],
    [3, 6, 2, 3, 6],
    [5, 1, 2, 4, 5],
    [2, 5, 2, 5, 3],
    [2, 2, 0, 0, 0],
    [1, 2, 2, 2, 1],
    [4, 2, 2, 2, 4],
    [0, 5, 2, 5, 0],
    [0, 2, 7, 2, 0],
    [0, 0, 0, 2, 4],
    [0, 0, 7, 0, 0],
    [0, 0, 0, 0, 2],
    [1, 1, 2, 4, 4],
    [7, 5, 5, 5, 7],
    [2, 6, 2, 2, 7],
    [6, 1, 2, 4, 7],
    [6, 1, 2, 1, 6],
    [5, 5, 7, 1, 1],
    [7, 4, 6, 1, 6],
    [3, 4, 6, 5, 2],
    [7, 1, 2, 2, 2],
    [2, 5, 2, 5, 2],
    [2, 5, 3, 1, 6],
    [0, 2, 0, 2, 0],
    [0, 2, 0, 2, 4],
    [1, 2, 4, 2, 1],
    [0, 7, 0, 7, 0],
    [4, 2, 1, 2, 4],
    [6, 1, 2, 0, 2],
    [2, 5, 7, 4, 3],
    [2, 5, 7, 5, 5],
    [6, 5, 6, 5, 6],
    [3, 4, 4, 4, 3],
    [6, 5, 5, 5, 6],
    [7, 4, 6, 4, 7],
    [7, 4, 6, 4, 4],
    [3, 4, 5, 5, 3],
    [5, 5, 7, 5, 5],
    [7, 2, 2, 2, 7],
    [1, 1, 1, 5, 2],
    [5, 5, 6, 5, 5],
    [4, 4, 4, 4, 7],
    [5, 7, 7, 5, 5],
    [5, 7, 7, 7, 5],
    [2, 5, 5, 5, 2],
    [6, 5, 6, 4, 4],
    [2, 5, 5, 3, 1],
    [6, 5, 6, 5, 5],
    [3, 4, 2, 1, 6],
    [7, 2, 2, 2, 2],
    [5, 5, 5, 5, 7],
    [5, 5, 5, 5, 2],
    [5, 5, 7, 7, 5],
    [5, 5, 2, 5, 5],
    [5, 5, 2, 2, 2],
    [7, 1, 2, 4, 7],
    [3, 2, 2, 2, 3],
    [4, 4, 2, 1, 1],
    [6, 2, 2, 2, 6],
    [2, 5, 0, 0, 0],
    [0, 0, 0, 0, 7],
    [4, 2, 0, 0, 0],
    [0, 3, 5, 5, 3],
    [4, 4, 6, 5, 6],
    [0, 3, 4, 4, 3],
    [1, 1, 3, 5, 3],
    [0, 2, 5, 6, 3],
    [1, 2, 7, 2, 2],
    [0, 3, 5, 3, 6],
    [4, 4, 6, 5, 5],
    [2, 0, 2, 2, 2],
    [1, 0, 1, 5, 2],
    [4, 5, 6, 6, 5],
    [6, 2, 2, 2, 3],
    [0, 5, 7, 7, 5],
    [0, 6, 5, 5, 5],
    [0, 2, 5, 5, 2],
    [0, 6, 5, 6, 4],
    [0, 3, 5, 3, 1],
    [0, 3, 4, 4, 4],
    [0, 3, 6, 3, 6],
    [2, 7, 2, 2, 3],
    [0, 5, 5, 5, 3],
    [0, 5, 5, 5, 2],
    [0, 5, 7, 7, 5],
    [0, 5, 2, 2, 5],
    [0, 5, 5, 3, 6],
    [0, 7, 1, 4, 7],
    [3, 2, 6, 2, 3],
    [2, 2, 2, 2, 2],
    [6, 2, 3, 2, 6],
    [0, 3, 6, 0, 0],
];
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlyphRun {
    pub x: usize,
    pub y: usize,
    pub width: usize,
}
#[derive(Default)]
pub struct GlyphCache {
    cache: HashMap<(u8, usize, usize), Arc<[GlyphRun]>>,
    order: VecDeque<(u8, usize, usize)>,
}
impl GlyphCache {
    pub fn get(&mut self, byte: u8, width: usize, height: usize) -> Arc<[GlyphRun]> {
        let byte = if (32..=126).contains(&byte) { byte } else { 46 };
        let width = width.clamp(1, 64);
        let height = height.clamp(1, 64);
        let key = (byte, width, height);
        if let Some(runs) = self.cache.get(&key) {
            return runs.clone();
        }
        let source = FONT[(byte - 32) as usize];
        let mut pixels = vec![false; width * height];
        let mut strongest = None;
        let mut best = 0.0;
        for y in 0..height {
            let top = y as f64 * 5.0 / height as f64;
            let bottom = (y + 1) as f64 * 5.0 / height as f64;
            for x in 0..width {
                let left = x as f64 * 3.0 / width as f64;
                let right = (x + 1) as f64 * 3.0 / width as f64;
                let mut coverage = 0.0;
                for sy in top.floor() as usize..bottom.ceil() as usize {
                    for sx in left.floor() as usize..right.ceil() as usize {
                        if source[sy] & (1 << (2 - sx)) != 0 {
                            coverage += (bottom.min((sy + 1) as f64) - top.max(sy as f64))
                                * (right.min((sx + 1) as f64) - left.max(sx as f64));
                        }
                    }
                }
                coverage /= (bottom - top) * (right - left);
                let index = y * width + x;
                pixels[index] = coverage >= 0.5;
                if coverage > best {
                    best = coverage;
                    strongest = Some(index);
                }
            }
        }
        if !pixels.iter().any(|p| *p) {
            if let Some(index) = strongest {
                pixels[index] = true;
            }
        }
        let mut runs = vec![];
        for y in 0..height {
            let mut x = 0;
            while x < width {
                if !pixels[y * width + x] {
                    x += 1;
                    continue;
                }
                let start = x;
                while x < width && pixels[y * width + x] {
                    x += 1;
                }
                runs.push(GlyphRun {
                    x: start,
                    y,
                    width: x - start,
                });
            }
        }
        if self.cache.len() >= 256 {
            if let Some(first) = self.order.pop_front() {
                self.cache.remove(&first);
            }
        }
        let runs: Arc<[GlyphRun]> = runs.into();
        self.order.push_back(key);
        self.cache.insert(key, runs.clone());
        runs
    }
    pub fn clear(&mut self) {
        self.cache.clear();
        self.order.clear();
    }
}
