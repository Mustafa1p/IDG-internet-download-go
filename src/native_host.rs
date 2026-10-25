//! Chrome's length-prefixed stdio bridge. Keep stdout exclusively for frames.
mod update_install;

use serde_json::{Value, json};
use std::io::{self, Read, Write};
use std::time::Duration;

fn read_message(input: &mut impl Read) -> io::Result<Value> {
    let mut length = [0; 4];
    input.read_exact(&mut length)?;
    let length = u32::from_ne_bytes(length) as usize;
    if length == 0 || length > 1024 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid frame length",
        ));
    }
    let mut bytes = vec![0; length];
    input.read_exact(&mut bytes)?;
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

fn write_message(output: &mut impl Write, value: &Value) -> io::Result<()> {
    let bytes = serde_json::to_vec(value)?;
    output.write_all(&(bytes.len() as u32).to_ne_bytes())?;
    output.write_all(&bytes)?;
    output.flush()
}

async fn is_online(client: &reqwest::Client) -> bool {
    let Ok(response) = client.get("http://127.0.0.1:17843/health").send().await else {
        return false;
    };
    if !response.status().is_success() {
        return false;
    }
    response
        .json::<Value>()
        .await
        .is_ok_and(|health| health["status"] == "ok" && health["version"].is_string())
}

async fn dispatch(client: &reqwest::Client, message: Value) -> anyhow::Result<Value> {
    let kind = message["type"].as_str().unwrap_or("");
    anyhow::ensure!(
        matches!(kind, "ping" | "download" | "list"),
        "Unknown request"
    );
    let base = "http://127.0.0.1:17843";
    let online = is_online(client).await;
    if !online && kind != "ping" {
        let exe = std::env::current_exe()?.with_file_name("IDG.exe");
        let mut command = std::process::Command::new(exe);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        let mut ready = false;
        for _ in 0..40 {
            tokio::time::sleep(Duration::from_millis(250)).await;
            if is_online(client).await {
                ready = true;
                break;
            }
        }
        anyhow::ensure!(ready, "IDG did not start. Open IDG and retry.");
    }
    if kind == "ping" {
        let tools = std::env::current_exe()?.with_file_name("tools");
        let youtube = ["yt-dlp.exe", "ffmpeg.exe", "ffprobe.exe"]
            .iter()
            .all(|name| tools.join(name).is_file())
            && (tools.join("deno.exe").is_file() || tools.join("node.exe").is_file());
        let music = client.get(format!("{base}/health")).send().await.ok();
        let music = if let Some(response) = music {
            response
                .json::<Value>()
                .await
                .ok()
                .is_some_and(|v| v["capabilities"]["music"] == true)
        } else {
            false
        };
        return Ok(
            json!({"ok": true, "installed": true, "serverOnline": online, "version": env!("CARGO_PKG_VERSION"), "capabilities": {"youtube": youtube, "x": youtube, "social_media": youtube, "music": music, "post_downloads": true}}),
        );
    }
    let response = if kind == "list" {
        client.get(format!("{base}/downloads")).send().await?
    } else {
        client
            .post(format!("{base}/downloads"))
            .json(&message["download"])
            .send()
            .await?
    };
    let body = response.error_for_status()?.json::<Value>().await?;
    Ok(json!({"ok": true, "data": body}))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("--apply-update") {
        let plan = std::env::args_os()
            .nth(2)
            .ok_or_else(|| anyhow::anyhow!("Update plan is missing"))?;
        return update_install::supervise(std::path::Path::new(&plan));
    }
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()?;
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    loop {
        let message = match read_message(&mut input) {
            Ok(message) => message,
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        };
        let response = dispatch(&client, message)
            .await
            .unwrap_or_else(|e| json!({"ok": false, "error": e.to_string()}));
        write_message(&mut output, &response)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frames_round_trip_and_reject_oversized_messages() {
        let value = json!({"type": "download", "filename": "ملف.zip"});
        let mut bytes = Vec::new();
        write_message(&mut bytes, &value).unwrap();
        assert_eq!(read_message(&mut bytes.as_slice()).unwrap(), value);
        assert!(read_message(&mut (2_000_000u32.to_ne_bytes().as_slice())).is_err());
    }
}
