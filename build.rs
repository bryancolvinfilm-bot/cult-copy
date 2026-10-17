fn main() {
    println!("cargo:rerun-if-changed=packaging/windows.rc");
    println!("cargo:rerun-if-changed=packaging/icons/duet-clipboard.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile_for(
            "packaging/windows.rc",
            ["duet-clipboard"],
            embed_resource::NONE,
        )
        .manifest_required()
        .expect("Windows executable icon must be embedded");
    }
}
