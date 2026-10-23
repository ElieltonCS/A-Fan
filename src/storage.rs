use crate::core::Settings;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn data_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("A-Fan")
}
pub fn settings_path() -> PathBuf {
    data_dir().join("settings.json")
}
pub fn load() -> Result<Settings, String> {
    let path = settings_path();
    load_from(&path)
}
fn load_from(path: &Path) -> Result<Settings, String> {
    if !path.exists() {
        return Ok(Settings::default());
    }
    use std::io::Read;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(16385)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 16384 {
        return Err("Arquivo de configurações excede 16 KB.".into());
    }
    let settings: Settings =
        serde_json::from_slice(&bytes).map_err(|e| format!("Configurações inválidas: {e}"))?;
    settings.validate()?;
    Ok(settings)
}
pub fn save(settings: &Settings) -> Result<(), String> {
    save_to(&settings_path(), settings)
}
fn save_to(path: &Path, settings: &Settings) -> Result<(), String> {
    settings.validate()?;
    write_json(path, settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_curve_does_not_replace_saved_configuration() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "a-fan-settings-{}-{unique}.json",
            std::process::id()
        ));
        let mut settings = Settings::default();
        save_to(&path, &settings).unwrap();
        let original = fs::read(&path).unwrap();
        settings.custom_curve.points[1].percent = 0;
        assert!(save_to(&path, &settings).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        assert!(load_from(&path).is_ok());
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn oversized_and_unknown_settings_are_rejected() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "a-fan-invalid-{}-{unique}.json",
            std::process::id()
        ));
        fs::write(&path, vec![b' '; 16385]).unwrap();
        assert!(load_from(&path).is_err());
        let settings = Settings {
            version: 2,
            ..Settings::default()
        };
        write_json(&path, &settings).unwrap();
        assert!(load_from(&path).is_err());
        fs::remove_file(path).unwrap();
    }
}
pub fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let temporary = path.with_extension("json.tmp");
    {
        use std::io::Write;
        let mut f = fs::File::create(&temporary).map_err(|e| e.to_string())?;
        f.write_all(&bytes)
            .and_then(|_| f.sync_all())
            .map_err(|e| e.to_string())?;
    }
    #[cfg(windows)]
    unsafe {
        use windows::{
            Win32::Storage::FileSystem::{
                MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
            },
            core::PCWSTR,
        };
        let a: Vec<u16> = temporary
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let b: Vec<u16> = path
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain(Some(0))
            .collect();
        MoveFileExW(
            PCWSTR(a.as_ptr()),
            PCWSTR(b.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
        .map_err(|e| e.to_string())?;
    }
    #[cfg(not(windows))]
    fs::rename(temporary, path).map_err(|e| e.to_string())?;
    Ok(())
}
