# Embedded HexForge fonts

These are static 400/600/700 instances of the exact JetBrains Mono 2.211 variable font used by the reference interface (`src/assets/fonts/JetBrainsMono.woff2`), not substitute fonts or newer upstream glyphs. Original font SHA-256: `18be452724bfdc236c074ca94a249a7f41a86752c7d04ab258ce9ed5651f6a7e`.

Generated with fonttools 4.66.1: WOFF2 decompression, then static `wght=400`, `wght=600`, and `wght=700` instances without recalculating timestamps. All three are embedded into the binary. `scripts/generate-native-fonts.py` reproduces the conversion from the recorded original; no download or runtime generator is required. Copyright 2020 The JetBrains Mono Project Authors. SIL Open Font License 1.1 is included in `OFL.txt`.

The symbols-only fallback chain follows the original regular/semibold fonts; it never replaces interface typography. The reference's Latin webfont lacks these symbols and the browser rendered them through OS fallback. Actual character maps and rendered glyphs were checked, rather than relying on egui's face-level `has_glyph` predicate.

- Noto Sans Symbols supplies U+2303/U+2304 chevrons and arrows. Source: https://notofonts.github.io/symbols/fonts/NotoSansSymbols/hinted/ttf/NotoSansSymbols-Regular.ttf; SHA-256 `283f7b0d9626a2d2847810184bc38d137f370d8a5d535cc84b05e8553fac7f4d`.
- Noto Sans Symbols2 supplies disclosure triangles and checkmarks. Source: https://notofonts.github.io/symbols/fonts/NotoSansSymbols2/hinted/ttf/NotoSansSymbols2-Regular.ttf; SHA-256 `5b5c3d6e3bfc74d882c89363a5b68aac0af68ef43626ce9b746eda340325c2fc`.
- Noto Sans Math supplies U+29C9 (Duplicate), absent from both symbol fonts. Source: https://notofonts.github.io/math/fonts/NotoSansMath/hinted/ttf/NotoSansMath-Regular.ttf; SHA-256 `f636075270f542fcb7f1e46f395e9328eab8e4ddf5477ccd8f88637a412bdf25`.

All are unmodified upstream files, copyright 2022 The Noto Project Authors, SIL OFL 1.1. Licenses are retained in `NotoSymbols-OFL.txt` and `NotoMath-OFL.txt`. Fonts are compiled into the executable, requiring no runtime font sidecars. Symbol outlines may differ slightly from the browser's OS fallback; paired visual verification must document this permitted rasterization/fallback difference.
