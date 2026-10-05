use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use crate::core::error::{ResonaError, Result};
use crate::core::project::Project;
use super::state::{clean_path, MultiSpecItem, MultiSpecState, StackLayoutMode};

fn default_fixed_slot_height() -> f32 {
    140.0
}

/// RSM アーカイブの全体メタデータ (multispec.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RsmManifest {
    pub version: u32,
    #[serde(default)]
    pub title: String,
    pub common_ppm_min: f64,
    pub common_ppm_max: f64,
    pub stack_spacing: f64,
    #[serde(default)]
    pub is_overlay: bool,
    #[serde(default)]
    pub stack_mode: StackLayoutMode,
    #[serde(default = "default_fixed_slot_height")]
    pub fixed_slot_height: f32,
    pub selected_id: Option<String>,
    pub items: Vec<RsmItemEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RsmItemEntry {
    pub id: String,
    pub name: String,
    pub source_path: Option<PathBuf>,
    pub color: [u8; 3],
    pub visible: bool,
    pub y_scale_max: f64,
    pub y_scale_min: f64,
    pub y_offset: f64,
    pub show_integral: bool,
    pub rsn_archive_name: String,
}

/// 単一アイテムをディスクの参照先ファイルから最新化する
/// 参照先ファイルが存在し正常に読み込めた場合は true を返す
pub fn reload_item_from_disk(item: &mut MultiSpecItem) -> bool {
    if let Some(ref path) = item.source_path {
        if path.exists() {
            let mut proj = Project::new();
            if proj.load_rsn(path).is_ok() {
                item.project = Some(proj);
                return true;
            }
        }
    }
    false
}

/// 全アイテムを参照先ファイルから最新化する (更新されたアイテム数を返す)
pub fn reload_all_from_disk(state: &mut MultiSpecState) -> usize {
    let mut updated = 0;
    for item in &mut state.items {
        if reload_item_from_disk(item) {
            updated += 1;
        }
    }
    if updated > 0 {
        state.update_common_ppm_range();
        state.push_history();
    }
    updated
}

/// MultiSpecState を .rsm ファイル (ZIPアーカイブ) として保存する
pub fn save_rsm<P: AsRef<Path>>(state: &mut MultiSpecState, path: P) -> Result<()> {
    // 1. 保存前に参照先ファイルが存在するアイテムは最新データで内部データを更新
    for item in &mut state.items {
        reload_item_from_disk(item);
    }

    let p = path.as_ref();
    if let Some(parent) = p.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            let _ = std::fs::create_dir_all(parent);
        }
    }

    let file = File::create(p)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let temp_dir = std::env::temp_dir();
    let temp_rsn_path = temp_dir.join(format!("resona_temp_{}.rsn", std::process::id()));

    let mut item_entries = Vec::new();

    // 2. 各アイテムの RSN データをアーカイブ内に書き込む
    for (idx, item) in state.items.iter().enumerate() {
        let rsn_name = format!("items/item_{}_{}.rsn", idx, item.id);
        if let Some(ref proj) = item.project {
            // 一時ファイルに RSN を保存し、ZIP に格納
            proj.save_rsn(&temp_rsn_path)?;
            let mut rsn_file = File::open(&temp_rsn_path)?;
            let mut rsn_bytes = Vec::new();
            rsn_file.read_to_end(&mut rsn_bytes)?;
            let _ = std::fs::remove_file(&temp_rsn_path);

            zip.start_file(&rsn_name, options)?;
            zip.write_all(&rsn_bytes)?;
        }

        item_entries.push(RsmItemEntry {
            id: item.id.clone(),
            name: item.name.clone(),
            source_path: item.source_path.clone(),
            color: item.color,
            visible: item.visible,
            y_scale_max: item.y_scale_max,
            y_scale_min: item.y_scale_min,
            y_offset: item.y_offset,
            show_integral: item.show_integral,
            rsn_archive_name: rsn_name,
        });
    }

    // 3. multispec.json を書き込む
    let manifest = RsmManifest {
        version: 1,
        title: state.title.clone(),
        common_ppm_min: state.common_ppm_min,
        common_ppm_max: state.common_ppm_max,
        stack_spacing: state.stack_spacing,
        is_overlay: state.is_overlay,
        stack_mode: state.stack_mode,
        fixed_slot_height: state.fixed_slot_height,
        selected_id: state.selected_id.clone(),
        items: item_entries,
    };

    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| ResonaError::ProcessingError(format!("RSM manifest JSON error: {}", e)))?;

    zip.start_file("multispec.json", options)?;
    zip.write_all(manifest_json.as_bytes())?;

    zip.finish()?;
    state.rsm_path = Some(clean_path(p));
    Ok(())
}

/// .rsm ファイル (ZIPアーカイブ) から MultiSpecState を復元する
pub fn load_rsm<P: AsRef<Path>>(path: P) -> Result<MultiSpecState> {
    let p = path.as_ref();
    let file = File::open(p)?;
    let mut zip = ZipArchive::new(file)?;

    // 1. multispec.json を読み込む
    let manifest: RsmManifest = {
        let mut manifest_file = zip.by_name("multispec.json")
            .map_err(|_| ResonaError::ProcessingError("Invalid RSM: missing multispec.json".to_string()))?;
        let mut content = String::new();
        manifest_file.read_to_string(&mut content)?;
        serde_json::from_str(&content)
            .map_err(|e| ResonaError::ProcessingError(format!("Failed to parse multispec.json: {}", e)))?
    };

    let temp_dir = std::env::temp_dir();
    let mut items = Vec::new();

    // 2. 各アイテムの復元
    for entry in manifest.items {
        let mut loaded_project = None;
        let cleaned_source_path = entry.source_path.map(|p| clean_path(&p));

        // 優先度 1: 参照先パスが存在する場合はそこから最新 RSN を読み込む
        if let Some(ref source_path) = cleaned_source_path {
            if source_path.exists() {
                let mut proj = Project::new();
                if proj.load_rsn(source_path).is_ok() {
                    loaded_project = Some(proj);
                }
            }
        }

        // 優先度 2: 参照先がない場合は ZIP 内の内部 RSN から復元
        if loaded_project.is_none() {
            if let Ok(mut rsn_entry) = zip.by_name(&entry.rsn_archive_name) {
                let mut rsn_bytes = Vec::new();
                rsn_entry.read_to_end(&mut rsn_bytes)?;
                let temp_path = temp_dir.join(format!("resona_rsm_extract_{}_{}.rsn", std::process::id(), entry.id));
                {
                    let mut temp_file = File::create(&temp_path)?;
                    temp_file.write_all(&rsn_bytes)?;
                }
                let mut proj = Project::new();
                let load_res = proj.load_rsn(&temp_path);
                let _ = std::fs::remove_file(&temp_path);
                if load_res.is_ok() {
                    loaded_project = Some(proj);
                }
            }
        }

        if let Some(proj) = loaded_project {
            items.push(MultiSpecItem {
                id: entry.id,
                name: entry.name,
                source_path: cleaned_source_path,
                color: entry.color,
                visible: entry.visible,
                y_scale_max: entry.y_scale_max,
                y_scale_min: entry.y_scale_min,
                y_offset: entry.y_offset,
                show_integral: entry.show_integral,
                project: Some(proj),
            });
        }
    }

    let mut state = MultiSpecState {
        title: manifest.title,
        items,
        selected_id: manifest.selected_id,
        common_ppm_min: manifest.common_ppm_min,
        common_ppm_max: manifest.common_ppm_max,
        stack_spacing: manifest.stack_spacing,
        is_overlay: manifest.is_overlay,
        stack_mode: manifest.stack_mode,
        fixed_slot_height: manifest.fixed_slot_height,
        zoom_history: Vec::new(),
        history: crate::multispec::state::MultiSpecHistory::new(),
        next_item_num: 1,
        rsm_path: Some(clean_path(p)),
    };

    if state.selected_id.is_none() {
        state.selected_id = state.items.first().map(|it| it.id.clone());
    }

    state.push_history();
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::Array1;

    fn create_test_project() -> Project {
        let mut proj = Project::new();
        proj.ppm = Some(Array1::from_vec(vec![10.0, 5.0, 0.0]));
        proj.spectrum_real = Some(Array1::from_vec(vec![0.0, 50.0, 0.0]));
        proj
    }

    #[test]
    fn test_rsm_save_and_load_roundtrip() {
        let temp_dir = std::env::temp_dir();
        let rsn_path = temp_dir.join(format!("test_multi_{}.rsn", std::process::id()));
        let rsm_path = temp_dir.join(format!("test_multi_{}.rsm", std::process::id()));

        // 1. RSN ファイルを一時作成
        let p1 = create_test_project();
        p1.save_rsn(&rsn_path).unwrap();

        // 2. MultiSpecState に追加して RSM 保存
        let mut state = MultiSpecState::new();
        state.add_rsn_file(&rsn_path).unwrap();
        save_rsm(&mut state, &rsm_path).unwrap();

        // 3. RSM 読み込み
        let loaded = load_rsm(&rsm_path).unwrap();
        assert_eq!(loaded.items.len(), 1);
        assert_eq!(loaded.items[0].name, rsn_path.file_stem().unwrap().to_str().unwrap());
        assert!(loaded.items[0].project.is_some());

        // 4. 元 RSN ファイルを削除しても、RSM 内部のコピーから復元できるか検証
        let _ = std::fs::remove_file(&rsn_path);
        let fallback_loaded = load_rsm(&rsm_path).unwrap();
        assert_eq!(fallback_loaded.items.len(), 1);
        assert!(fallback_loaded.items[0].project.is_some());

        let _ = std::fs::remove_file(&rsm_path);
    }
}
