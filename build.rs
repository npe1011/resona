use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    println!("cargo:rerun-if-changed=src/icon/icon_windows.ico");

    if target_os == "windows" {
        let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
        let ico_path = PathBuf::from(&manifest_dir).join("src").join("icon").join("icon_windows.ico");
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let rc_path = out_dir.join("resona.rc");
        let res_path = out_dir.join("resona.res");

        let rc_content = format!("1 ICON \"{}\"\n", ico_path.to_string_lossy().replace('\\', "\\\\"));
        if let Err(e) = std::fs::write(&rc_path, rc_content) {
            eprintln!("cargo:warning=Failed to write resona.rc: {}", e);
            return;
        }

        if target_env == "msvc" {
            let mut compiled = false;
            // 1. Try rc.exe in PATH
            if let Ok(st) = Command::new("rc.exe")
                .arg(format!("/fo{}", res_path.to_str().unwrap()))
                .arg(rc_path.to_str().unwrap())
                .status()
            {
                if st.success() {
                    compiled = true;
                }
            }

            // 2. Try Windows Kits fallback
            if !compiled {
                let kit_rc = "C:\\Program Files (x86)\\Windows Kits\\10\\bin\\10.0.26100.0\\x64\\rc.exe";
                if let Ok(st) = Command::new(kit_rc)
                    .arg(format!("/fo{}", res_path.to_str().unwrap()))
                    .arg(rc_path.to_str().unwrap())
                    .status()
                {
                    if st.success() {
                        compiled = true;
                    }
                }
            }

            if compiled {
                println!("cargo:rustc-link-arg={}", res_path.to_str().unwrap());
            } else {
                eprintln!("cargo:warning=rc.exe compilation was not successful; icon not embedded in exe");
            }
        } else if target_env == "gnu" {
            if let Ok(st) = Command::new("windres")
                .arg(&rc_path)
                .arg("-O")
                .arg("coff")
                .arg("-o")
                .arg(&res_path)
                .status()
            {
                if st.success() {
                    println!("cargo:rustc-link-arg={}", res_path.to_str().unwrap());
                }
            }
        }
    }
}
