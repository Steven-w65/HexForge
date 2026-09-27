fn main() {
    // A direct `cargo build` must re-embed freshly built Vite assets.
    println!("cargo:rerun-if-changed=../dist");
    tauri_build::build()
}
