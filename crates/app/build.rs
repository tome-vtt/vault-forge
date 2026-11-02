fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=icons/vault-forge-arcane.ico");

    // Build scripts run on the host; resource embedding depends on the target.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("icons/vault-forge-arcane.ico")
            .compile()
            .expect("could not embed the Windows program icon");
    }
}
