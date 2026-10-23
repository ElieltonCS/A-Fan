use std::{env, path::PathBuf, process::Command};
fn main() {
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/app.manifest");
    println!("cargo:rerun-if-changed=assets/fan.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let root = PathBuf::from(env::var_os("ProgramFiles(x86)").expect("Windows SDK não localizado"))
        .join("Windows Kits/10/bin");
    let mut versions: Vec<_> = std::fs::read_dir(root)
        .expect("Instale o Windows SDK")
        .flatten()
        .map(|d| d.path())
        .collect();
    versions.sort();
    let rc = versions
        .into_iter()
        .rev()
        .map(|d| d.join("x64/rc.exe"))
        .find(|p| p.is_file())
        .expect("rc.exe não localizado no Windows SDK");
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("a-fan.res");
    let status = Command::new(rc)
        .args(["/nologo", "/I", "assets", "/fo"])
        .arg(&output)
        .arg("assets/app.rc")
        .status()
        .expect("Não foi possível executar rc.exe");
    assert!(status.success(), "Falha ao compilar recursos Windows");
    println!("cargo:rustc-link-arg={}", output.display());
}
