use super::minimap::MinimapDrawCache;
/// The minimap owns a viewport-sized raster. egui's bounded galley cache holds
/// the native hex font; no document-sized texture or shape list is retained.
#[derive(Default)]
pub struct HexDrawCache {
    pub minimap: MinimapDrawCache,
}
