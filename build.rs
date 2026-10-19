fn main() {
    println!("cargo:rerun-if-changed=assets/idg.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/idg.ico")
            .set("ProductName", "IDG – Internet Download GO")
            .set("FileDescription", "IDG – Internet Download GO")
            .compile()
            .expect("Could not embed Windows application icon");
    }
}
