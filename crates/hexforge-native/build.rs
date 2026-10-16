fn main() {
    println!("cargo:rerun-if-changed=assets/hexforge.rc");
    println!("cargo:rerun-if-changed=assets/hexforge.manifest");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("assets/hexforge.rc", embed_resource::NONE)
            .manifest_required()
            .expect("HexForge Windows resources must be embedded");
    }
}
