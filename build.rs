fn main() {
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    };
    let revision = git(&["rev-parse", "HEAD"]).unwrap_or_else(|| "source".into());
    let timestamp = git(&["show", "-s", "--format=%ct", "HEAD"]).unwrap_or_else(|| "0".into());
    println!("cargo:rustc-env=IDG_BUILD_ID={revision}");
    println!("cargo:rustc-env=IDG_BUILD_TIMESTAMP={timestamp}");
    for path in [
        Some(".git/HEAD".to_owned()),
        Some(".git/packed-refs".to_owned()),
        git(&["symbolic-ref", "-q", "HEAD"]).and_then(|r| git(&["rev-parse", "--git-path", &r])),
    ]
    .into_iter()
    .flatten()
    {
        println!("cargo:rerun-if-changed={path}");
    }

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
