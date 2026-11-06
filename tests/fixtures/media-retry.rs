// Deterministic subprocess fixture: the first CDN attempt fails with 403.
use std::{env, fs, path::PathBuf};
fn main() {
    let args: Vec<String> = env::args().collect();
    let tools = env::current_exe().unwrap().parent().unwrap().to_path_buf();
    let count = tools.join("attempts");
    let attempt: u32 = fs::read_to_string(&count).unwrap_or_default().parse().unwrap_or(0) + 1;
    fs::write(&count, attempt.to_string()).unwrap();
    assert!(args.iter().any(|arg| arg == "--abort-on-unavailable-fragments"));
    assert!(args.iter().any(|arg| arg.contains("height<=480")));
    let index = args.iter().position(|arg| arg == "--output").unwrap();
    let output = PathBuf::from(args[index + 1].replace("%%", "%").replace("%(ext)s", "mp4"));
    let partial = output.with_extension("mp4.part");
    if attempt == 1 || tools.join("always-fail").exists() {
        fs::write(&partial, b"retained partial bytes").unwrap();
        eprintln!("ERROR: unable to download video data: HTTP Error 403: Forbidden");
        eprintln!("ERROR: fragment 1 not found, unable to continue");
        std::process::exit(1);
    }
    assert_eq!(attempt, 2, "Recovery must be bounded");
    assert_eq!(fs::read(&partial).unwrap(), b"retained partial bytes");
    fs::rename(partial, &output).unwrap();
    // Temp paths created by this test contain no control characters.
    println!("IDG_FILE \"{}\"", output.to_string_lossy().replace('\\', "\\\\"));
}
