use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    // ビルド日 (YYYY-MM-DD) の算出と環境変数設定
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (now / 86400) as i64;
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1029 + doe / 1461 - doe / 36524) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let build_date = format!("{:04}-{:02}-{:02}", y, m, d);
    println!("cargo:rustc-env=RESONA_BUILD_DATE={}", build_date);

    if target_os == "windows" {
        println!("cargo:rerun-if-changed=assets/icons/icon_windows.ico");
        let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
        let ico_path = PathBuf::from(&manifest_dir).join("assets").join("icons").join("icon_windows.ico");
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let rc_path = out_dir.join("resona.rc");
        let res_path = out_dir.join("resona.res");

        let version_str = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_string());
        let version_parts: Vec<u32> = version_str
            .split('.')
            .filter_map(|s| s.parse::<u32>().ok())
            .collect();
        let major = version_parts.first().copied().unwrap_or(0);
        let minor = version_parts.get(1).copied().unwrap_or(0);
        let patch = version_parts.get(2).copied().unwrap_or(0);

        let rc_content = format!(
            "1 ICON \"{ico}\"\n\
            1 VERSIONINFO\n\
            FILEVERSION {major},{minor},{patch},0\n\
            PRODUCTVERSION {major},{minor},{patch},0\n\
            FILEFLAGSMASK 0x3fL\n\
            FILEFLAGS 0x0L\n\
            FILEOS 0x40004L\n\
            FILETYPE 0x1L\n\
            FILESUBTYPE 0x0L\n\
            BEGIN\n\
                BLOCK \"StringFileInfo\"\n\
                BEGIN\n\
                    BLOCK \"040904b0\"\n\
                    BEGIN\n\
                        VALUE \"FileDescription\", \"Resona - 1D NMR Analysis\\0\"\n\
                        VALUE \"FileVersion\", \"{version_str}\\0\"\n\
                        VALUE \"InternalName\", \"resona\\0\"\n\
                        VALUE \"LegalCopyright\", \"Copyright (c) 2026 Tatsuhiko Yoshino\\0\"\n\
                        VALUE \"OriginalFilename\", \"resona.exe\\0\"\n\
                        VALUE \"ProductName\", \"Resona\\0\"\n\
                        VALUE \"ProductVersion\", \"{version_str}\\0\"\n\
                    END\n\
                END\n\
                BLOCK \"VarFileInfo\"\n\
                BEGIN\n\
                    VALUE \"Translation\", 0x409, 1200\n\
                END\n\
            END\n",
            ico = ico_path.to_string_lossy().replace('\\', "\\\\"),
            major = major,
            minor = minor,
            patch = patch,
            version_str = version_str,
        );
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
