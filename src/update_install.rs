//! A temporary copy of the native bridge supervises installation after the GUI exits.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Deserialize, Serialize)]
pub struct Plan {
    pub directory: PathBuf,
    pub installer: PathBuf,
    pub parent_pid: u32,
    pub startup: bool,
}

pub fn installer_args(directory: &Path, startup: bool) -> Vec<String> {
    vec![
        "/SILENT".into(),
        "/NORESTART".into(),
        "/CLOSEAPPLICATIONS".into(),
        // The helper restarts IDG on success OR failure, after setup exits.
        "/IDGUPDATE=helper".into(),
        format!("/DIR={}", directory.display()),
        format!("/TASKS={}", if startup { "startupicon" } else { "" }),
    ]
}

#[cfg(windows)]
pub fn supervise(plan_file: &Path) -> Result<()> {
    use std::{
        os::windows::process::CommandExt,
        process::{Command, Stdio},
    };
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };
    let plan: Plan = serde_json::from_slice(&std::fs::read(plan_file)?)?;
    ensure!(
        plan.installer.is_file() && plan.directory.join("IDG.exe").is_file(),
        "Update files are missing"
    );
    // SAFETY: only wait on the originating IDG process; no process is terminated.
    let parent = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, plan.parent_pid) };
    ensure!(!parent.is_null(), "Could not wait for IDG to close");
    std::fs::write(plan_file.with_extension("ready"), b"ready")?;
    let waited = unsafe { WaitForSingleObject(parent, 30_000) };
    unsafe {
        CloseHandle(parent);
    }
    ensure!(waited == 0, "IDG did not close; update was not installed");
    let result = Command::new(&plan.installer)
        .args(installer_args(&plan.directory, plan.startup))
        .arg(format!(
            "/LOG={}",
            plan_file.with_extension("log").display()
        ))
        .creation_flags(0x08000000)
        .status();
    let error = match result {
        Ok(status) if status.success() => None,
        Ok(status) => Some(format!(
            "The update installer exited with code {:?}. Your app has been reopened. Check the installation folder permissions and retry. Log: {}",
            status.code(),
            plan_file.with_extension("log").display()
        )),
        Err(error) => Some(format!(
            "The update installer could not start: {error}. Your app has been reopened; retry the update."
        )),
    };
    if let Some(error) = error {
        if let Some(config) = dirs::config_dir() {
            let directory = config.join("IDG");
            let _ = std::fs::create_dir_all(&directory);
            let _ = std::fs::write(directory.join("update-error.txt"), error);
        }
    } else {
        // The large installer is no longer needed. The small running helper is
        // left in its private temporary folder because Windows locks its image.
        let _ = std::fs::remove_file(&plan.installer);
    }
    Command::new(plan.directory.join("IDG.exe"))
        .creation_flags(0x08000000)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("Could not reopen IDG after installation")?;
    Ok(())
}

#[cfg(not(windows))]
pub fn supervise(_plan_file: &Path) -> Result<()> {
    anyhow::bail!("Updates require Windows");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn installer_preserves_directory_and_startup_choice_without_shell_parsing() {
        let args = installer_args(Path::new(r"C:\IDG Files"), true);
        assert!(args.contains(&r"/DIR=C:\IDG Files".into()));
        assert!(args.contains(&"/TASKS=startupicon".into()));
        assert!(installer_args(Path::new("."), false).contains(&"/TASKS=".into()));
    }
}
