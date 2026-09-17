# HexForge

A modern, lightweight hex viewer and binary analysis tool. HexForge is fully offline and local-first, with custom parsing templates, in-memory byte editing, paginated large-file loading, and a VS Code-style GUI.

## Development

Install dependencies with `npm install`, then use the project scripts:

- `npm run dev` starts the Vite frontend.
- `npm test` runs frontend tests.
- `npm run typecheck` checks TypeScript and Vue types.
- `cargo test --manifest-path src-tauri/Cargo.toml` runs Rust tests.
