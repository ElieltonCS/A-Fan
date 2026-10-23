use std::{os::windows::process::CommandExt, path::PathBuf, process::Command};
use windows::{
    Win32::{
        Foundation::*,
        System::{Registry::*, Threading::*},
        UI::{Shell::IsUserAnAdmin, WindowsAndMessaging::*},
    },
    core::{PCWSTR, w},
};
pub fn error(message: &str) {
    unsafe {
        let text = crate::hardware::wide(message);
        let _ = MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            windows::core::w!("A-Fan"),
            MB_ICONERROR | MB_OK,
        );
    }
}
pub fn elevated() -> bool {
    unsafe { IsUserAnAdmin().as_bool() }
}

pub struct OwnedHandle(pub HANDLE);
unsafe impl Send for OwnedHandle {}
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
pub fn event(name: &str) -> Result<OwnedHandle, String> {
    let name = crate::hardware::wide(name);
    unsafe { CreateEventW(None, false, false, PCWSTR(name.as_ptr())) }
        .map(OwnedHandle)
        .map_err(|e| e.to_string())
}
pub fn open_event(name: &str) -> Result<OwnedHandle, String> {
    let name = crate::hardware::wide(name);
    unsafe { OpenEventW(EVENT_ALL_ACCESS, false, PCWSTR(name.as_ptr())) }
        .map(OwnedHandle)
        .map_err(|e| e.to_string())
}
pub fn signal(event: &OwnedHandle) -> Result<(), String> {
    unsafe { SetEvent(event.0) }.map_err(|e| e.to_string())
}
pub fn signaled(event: &OwnedHandle) -> bool {
    unsafe { WaitForSingleObject(event.0, 0) == WAIT_OBJECT_0 }
}
pub fn installed_path() -> PathBuf {
    std::env::var_os("ProgramFiles")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\Program Files"))
        .join("A-Fan")
        .join("a-fan.exe")
}
pub fn installed() -> bool {
    installed_path().is_file()
}
pub fn startup_enabled() -> bool {
    let mut value = 0u32;
    let mut bytes = 4u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\A-Fan"),
            w!("AutoStart"),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut value as *mut u32).cast()),
            Some(&mut bytes),
        ) == ERROR_SUCCESS
            && value != 0
    }
}
fn powershell() -> Command {
    let root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\Windows"));
    let mut command = Command::new(root.join("System32\\WindowsPowerShell\\v1.0\\powershell.exe"));
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
    ]);
    command.creation_flags(CREATE_NO_WINDOW.0);
    command
}
pub fn launch_registered_task() -> Result<(), String> {
    let status = powershell()
        .args([
            "-Command",
            "Start-ScheduledTask -TaskName 'A-Fan' -ErrorAction Stop",
        ])
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("Não foi possível iniciar a tarefa A-Fan. Execute o instalador novamente.".into())
    }
}
pub fn set_startup(enabled: bool) -> Result<(), String> {
    let script = installed_path().parent().unwrap().join("set-startup.ps1");
    let status = powershell()
        .arg("-File")
        .arg(script)
        .arg("-Enabled")
        .arg(if enabled { "1" } else { "0" })
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("O Windows não confirmou a alteração da inicialização.".into())
    }
}
pub fn reveal_data_folder() {
    let _ = Command::new("explorer.exe")
        .arg(crate::storage::data_dir())
        .spawn();
}
