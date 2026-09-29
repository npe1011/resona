use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use byteorder::{BigEndian, ByteOrder, LittleEndian};
use ndarray::Array1;
use num_complex::Complex64;

use crate::core::error::{ResonaError, Result};
use crate::core::io::traits::{AcquisitionMetadata, NmrDataSource, RawFid};

/// JCAMP-DX パラメータ値の表現
#[derive(Debug, Clone, PartialEq)]
pub enum JcampValue {
    Number(f64),
    Text(String),
    Numbers(Vec<f64>),
    Texts(Vec<String>),
    Bool(bool),
}

impl JcampValue {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            JcampValue::Number(n) => Some(*n),
            JcampValue::Text(s) => s.parse::<f64>().ok(),
            _ => None,
        }
    }

    pub fn as_u32(&self) -> Option<u32> {
        self.as_f64().map(|n| n.round() as u32)
    }

    pub fn as_usize(&self) -> Option<usize> {
        self.as_f64().map(|n| n.round() as usize)
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            JcampValue::Text(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn get_f64_at(&self, idx: usize) -> Option<f64> {
        match self {
            JcampValue::Numbers(vec) => vec.get(idx).copied(),
            JcampValue::Number(n) if idx == 0 => Some(*n),
            _ => None,
        }
    }

    pub fn get_str_at(&self, idx: usize) -> Option<&str> {
        match self {
            JcampValue::Texts(vec) => vec.get(idx).map(|s| s.as_str()),
            JcampValue::Text(s) if idx == 0 => Some(s.as_str()),
            _ => None,
        }
    }
}

/// JCAMP-DX 形式のテキストをパースしてキーと値のマップを生成
pub fn parse_jcamp(content: &str) -> HashMap<String, JcampValue> {
    let mut params = HashMap::new();
    let mut lines = content.lines().peekable();

    while let Some(raw_line) = lines.next() {
        let line = raw_line.trim_end();
        if line.is_empty() || line.starts_with("$$") {
            continue;
        }
        if line.starts_with("##END=") || line == "##END" {
            break;
        }
        if !line.starts_with("##") {
            continue;
        }

        let without_hash = &line[2..];
        let without_dollar = if without_hash.starts_with('$') {
            &without_hash[1..]
        } else {
            without_hash
        };

        if let Some(eq_idx) = without_dollar.find('=') {
            let key = without_dollar[..eq_idx].trim().to_uppercase();
            let mut rest = without_dollar[eq_idx + 1..].trim_start().to_string();

            if rest.starts_with('<') {
                // 文字列 (複数行にわたる場合あり)
                while !rest.contains('>') {
                    if let Some(next_line) = lines.next() {
                        rest.push('\n');
                        rest.push_str(next_line.trim_end());
                    } else {
                        break;
                    }
                }
                let val = if let (Some(start), Some(end)) = (rest.find('<'), rest.rfind('>')) {
                    if end >= start + 1 {
                        &rest[start + 1..end]
                    } else {
                        ""
                    }
                } else {
                    rest.trim_matches(|c| c == '<' || c == '>')
                };
                params.insert(key, JcampValue::Text(val.to_string()));
            } else if rest.starts_with('(') {
                // 配列 (0..N)
                let inline_rest = if let Some(close_paren) = rest.find(')') {
                    rest[close_paren + 1..].trim().to_string()
                } else {
                    String::new()
                };

                let mut all_elements_text = inline_rest;
                while let Some(next_line) = lines.peek() {
                    let trimmed = next_line.trim();
                    if trimmed.starts_with("##") {
                        break;
                    }
                    if !trimmed.starts_with("$$") {
                        if !all_elements_text.is_empty() {
                            all_elements_text.push(' ');
                        }
                        all_elements_text.push_str(trimmed);
                    }
                    lines.next();
                }

                if all_elements_text.contains('<') {
                    // 文字列配列 (<...> で区切る)
                    let mut tokens = Vec::new();
                    let mut chars = all_elements_text.chars().peekable();
                    while let Some(c) = chars.next() {
                        if c == '<' {
                            let mut s = String::new();
                            while let Some(sc) = chars.next() {
                                if sc == '>' {
                                    break;
                                }
                                s.push(sc);
                            }
                            tokens.push(s);
                        }
                    }
                    params.insert(key, JcampValue::Texts(tokens));
                } else {
                    // 数値配列
                    let numbers: Vec<f64> = all_elements_text
                        .split_whitespace()
                        .filter_map(|t| t.parse::<f64>().ok())
                        .collect();
                    params.insert(key, JcampValue::Numbers(numbers));
                }
            } else if rest.eq_ignore_ascii_case("yes") {
                params.insert(key, JcampValue::Bool(true));
            } else if rest.eq_ignore_ascii_case("no") {
                params.insert(key, JcampValue::Bool(false));
            } else if let Ok(num) = rest.parse::<f64>() {
                params.insert(key, JcampValue::Number(num));
            } else {
                params.insert(key, JcampValue::Text(rest.trim().to_string()));
            }
        }
    }

    params
}

/// Bruker デジタルフィルター群遅延ルックアップテーブル (nmrglue 準拠)
pub fn lookup_bruker_dsp_delay(dspfvs: u32, decim: u32) -> Option<f64> {
    match dspfvs {
        10 => match decim {
            2 => Some(44.75),
            3 => Some(33.5),
            4 => Some(66.625),
            6 => Some(59.083333333333333),
            8 => Some(68.5625),
            12 => Some(60.375),
            16 => Some(69.53125),
            24 => Some(61.020833333333333),
            32 => Some(70.015625),
            48 => Some(61.34375),
            64 => Some(70.2578125),
            96 => Some(61.505208333333333),
            128 => Some(70.37890625),
            192 => Some(61.5859375),
            256 => Some(70.439453125),
            384 => Some(61.626302083333333),
            512 => Some(70.4697265625),
            768 => Some(61.646484375),
            1024 => Some(70.48486328125),
            1536 => Some(61.656575520833333),
            2048 => Some(70.492431640625),
            _ => None,
        },
        11 | 12 => match decim {
            2 => Some(46.0),
            3 => Some(36.5),
            4 => Some(48.0),
            6 => Some(50.166666666666667),
            8 => Some(53.25),
            12 => Some(69.5),
            16 => {
                if dspfvs == 11 {
                    Some(72.25)
                } else {
                    Some(71.625)
                }
            }
            24 => Some(70.166666666666667),
            32 => {
                if dspfvs == 11 {
                    Some(72.75)
                } else {
                    Some(72.125)
                }
            }
            48 => Some(70.5),
            64 => {
                if dspfvs == 11 {
                    Some(73.0)
                } else {
                    Some(72.375)
                }
            }
            96 => Some(70.666666666666667),
            128 => Some(72.5),
            192 => Some(71.333333333333333),
            256 => Some(72.25),
            384 => Some(71.666666666666667),
            512 => Some(72.125),
            768 => Some(71.833333333333333),
            1024 => Some(72.0625),
            1536 => Some(71.916666666666667),
            2048 => Some(72.03125),
            _ => None,
        },
        13 => match decim {
            2 => Some(2.75),
            3 => Some(2.8333333333333333),
            4 => Some(2.875),
            6 => Some(2.9166666666666667),
            8 => Some(2.9375),
            12 => Some(2.9583333333333333),
            16 => Some(2.96875),
            24 => Some(2.9791666666666667),
            32 => Some(2.984375),
            48 => Some(2.9895833333333333),
            64 => Some(2.9921875),
            96 => Some(2.9947916666666667),
            _ => None,
        },
        _ => None,
    }
}

/// Bruker デジタルフィルター群遅延の計算
pub fn compute_bruker_group_delay(
    grpdly: Option<f64>,
    dspfvs: Option<u32>,
    decim: Option<u32>,
) -> Option<f64> {
    if let Some(gd) = grpdly {
        if gd > 0.0 {
            return Some(gd);
        }
    }
    if let Some(fw) = dspfvs {
        if fw >= 14 {
            return Some(0.0);
        }
        if let Some(dec) = decim {
            return lookup_bruker_dsp_delay(fw, dec);
        }
    }
    None
}

/// UNIX タイムスタンプ秒を "YYYY-MM-DD HH:MM:SS" 形式にフォーマット
pub fn format_unix_timestamp(unix_seconds: f64) -> String {
    let total_secs = unix_seconds as i64;
    if total_secs <= 0 {
        return String::new();
    }

    let days = total_secs / 86400;
    let rem_secs = total_secs % 86400;
    let hour = rem_secs / 3600;
    let min = (rem_secs % 3600) / 60;
    let sec = rem_secs % 60;

    let mut y = 1970;
    let mut d = days;
    loop {
        let is_leap = (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0);
        let days_in_year = if is_leap { 366 } else { 365 };
        if d < days_in_year {
            break;
        }
        d -= days_in_year;
        y += 1;
    }

    let is_leap = (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0);
    let days_in_months = [
        31,
        if is_leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut m = 1;
    for &dim in &days_in_months {
        if d < dim {
            break;
        }
        d -= dim;
        m += 1;
    }
    let day = d + 1;

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        y, m, day, hour, min, sec
    )
}

/// Bruker データから解決されたパス群
#[derive(Debug, Clone, PartialEq)]
pub struct BrukerResolvedPaths {
    pub fid_path: PathBuf,
    pub acqus_path: PathBuf,
    pub title_path: Option<PathBuf>,
    pub audita_path: Option<PathBuf>,
}

/// Bruker データディレクトリやファイルから必要なパス群を解決
pub fn resolve_bruker_paths(path: &Path) -> Result<BrukerResolvedPaths> {
    if path.is_dir() {
        // 直下に fid があるか確認
        let fid_path = path.join("fid");
        if fid_path.is_file() {
            let acqus_path = if path.join("acqus").is_file() {
                path.join("acqus")
            } else if path.join("acqu").is_file() {
                path.join("acqu")
            } else {
                return Err(ResonaError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("acqus or acqu not found in {}", path.display()),
                )));
            };
            let title_path = path.join("pdata").join("1").join("title");
            let title_opt = if title_path.is_file() {
                Some(title_path)
            } else {
                None
            };
            let audita_path = path.join("audita.txt");
            let audita_opt = if audita_path.is_file() {
                Some(audita_path)
            } else {
                None
            };
            return Ok(BrukerResolvedPaths {
                fid_path,
                acqus_path,
                title_path: title_opt,
                audita_path: audita_opt,
            });
        }

        // TopSpin の expno フォルダ (例: 1/fid) を確認
        let exp1_fid = path.join("1").join("fid");
        if exp1_fid.is_file() {
            let acqus_path = if path.join("1").join("acqus").is_file() {
                path.join("1").join("acqus")
            } else {
                path.join("1").join("acqu")
            };
            let title_path = path.join("1").join("pdata").join("1").join("title");
            let title_opt = if title_path.is_file() {
                Some(title_path)
            } else {
                None
            };
            let audita_path = path.join("1").join("audita.txt");
            let audita_opt = if audita_path.is_file() {
                Some(audita_path)
            } else {
                None
            };
            return Ok(BrukerResolvedPaths {
                fid_path: exp1_fid,
                acqus_path,
                title_path: title_opt,
                audita_path: audita_opt,
            });
        }

        // 直下のサブディレクトリを探索
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                let sub = entry.path();
                if sub.is_dir() {
                    let sub_fid = sub.join("fid");
                    if sub_fid.is_file() {
                        let sub_acqus = if sub.join("acqus").is_file() {
                            sub.join("acqus")
                        } else {
                            sub.join("acqu")
                        };
                        let title_path = sub.join("pdata").join("1").join("title");
                        let title_opt = if title_path.is_file() {
                            Some(title_path)
                        } else {
                            None
                        };
                        let audita_path = sub.join("audita.txt");
                        let audita_opt = if audita_path.is_file() {
                            Some(audita_path)
                        } else {
                            None
                        };
                        return Ok(BrukerResolvedPaths {
                            fid_path: sub_fid,
                            acqus_path: sub_acqus,
                            title_path: title_opt,
                            audita_path: audita_opt,
                        });
                    }
                }
            }
        }

        Err(ResonaError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Bruker raw binary 'fid' not found in {}", path.display()),
        )))
    } else {
        // ファイルが直接指定された場合
        let file_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        let parent = path.parent().unwrap_or_else(|| Path::new("."));

        let (fid_path, acqus_path) = if file_name == "fid" {
            let acqus = if parent.join("acqus").is_file() {
                parent.join("acqus")
            } else if parent.join("acqu").is_file() {
                parent.join("acqu")
            } else {
                return Err(ResonaError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("acqus or acqu not found in {}", parent.display()),
                )));
            };
            (path.to_path_buf(), acqus)
        } else if file_name == "acqus" || file_name == "acqu" {
            let fid = parent.join("fid");
            if !fid.is_file() {
                return Err(ResonaError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("fid not found in {}", parent.display()),
                )));
            }
            (fid, path.to_path_buf())
        } else {
            // その他のファイルの場合、同じフォルダに fid と acqus があるか確認
            let fid = parent.join("fid");
            let acqus = if parent.join("acqus").is_file() {
                parent.join("acqus")
            } else {
                parent.join("acqu")
            };
            if fid.is_file() && acqus.is_file() {
                (fid, acqus)
            } else {
                return Err(ResonaError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Cannot find Bruker fid/acqus for {}", path.display()),
                )));
            }
        };

        let title_path = parent.join("pdata").join("1").join("title");
        let title_opt = if title_path.is_file() {
            Some(title_path)
        } else {
            None
        };
        let audita_path = parent.join("audita.txt");
        let audita_opt = if audita_path.is_file() {
            Some(audita_path)
        } else {
            None
        };
        Ok(BrukerResolvedPaths {
            fid_path,
            acqus_path,
            title_path: title_opt,
            audita_path: audita_opt,
        })
    }
}

/// タイトル文字列が JCAMP-DX や TopSpin の定型ヘッダー・デフォルト文字列かどうかを判定する
pub fn is_generic_bruker_title(s: &str) -> bool {
    let lower = s.to_lowercase();
    let trimmed = lower.trim();
    trimmed.is_empty()
        || trimmed.starts_with("parameter file")
        || trimmed.starts_with("audit trail")
        || trimmed.starts_with("bruker bio")
        || trimmed == "topspin"
        || trimmed.starts_with("topspin ")
        || trimmed == "untitled"
}

/// audita.txt からデータセット名（実験名）を抽出する
pub fn extract_title_from_audita(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        // 例: "$$ /opt/data/thosjl/PMP-vinyl-Indoline-sub-SI/13/audita.txt"
        // または "$$ C:\data\thosjl\PMP-vinyl-Indoline-sub-SI\13\audita.txt"
        if trimmed.starts_with("$$")
            && (trimmed.ends_with("/audita.txt") || trimmed.ends_with(r"\audita.txt"))
        {
            let path_part = trimmed.trim_start_matches('$').trim();
            let parts: Vec<&str> = path_part
                .split(|c| c == '/' || c == '\\')
                .filter(|s| !s.is_empty())
                .collect();
            // 末尾が "audita.txt"
            if parts.len() >= 3 && parts.last().copied() == Some("audita.txt") {
                let expno_cand = parts[parts.len() - 2];
                if expno_cand.chars().all(|c| c.is_ascii_digit()) {
                    let dataset_cand = parts[parts.len() - 3];
                    if !dataset_cand.is_empty() {
                        return Some(dataset_cand.to_string());
                    }
                } else if !expno_cand.is_empty() {
                    return Some(expno_cand.to_string());
                }
            } else if parts.len() >= 2 && parts.last().copied() == Some("audita.txt") {
                let prev = parts[parts.len() - 2];
                if !prev.is_empty() {
                    return Some(prev.to_string());
                }
            }
        }
    }
    None
}

/// ファイルまたはディレクトリパスから適切なデータセット名（フォールバック用）を抽出する
pub fn fallback_dataset_name(path: &Path) -> String {
    let dir = if path.is_file() {
        path.parent().unwrap_or(path)
    } else if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
        let lower = file_name.to_lowercase();
        if lower == "fid"
            || lower == "acqus"
            || lower == "acqu"
            || lower == "audita.txt"
            || path.extension().is_some()
        {
            path.parent().unwrap_or(path)
        } else {
            path
        }
    } else {
        path
    };

    let name = dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    // ディレクトリ名が数値（TopSpin の expno: 例 1, 10, 13）の場合は、親ディレクトリ名を採用
    if !name.is_empty() && name.chars().all(|c| c.is_ascii_digit()) {
        if let Some(parent) = dir.parent() {
            if let Some(parent_name) = parent.file_name().and_then(|s| s.to_str()) {
                if !parent_name.is_empty() {
                    return parent_name.to_string();
                }
            }
        }
    }

    if !name.is_empty() {
        name.to_string()
    } else {
        "Bruker NMR".to_string()
    }
}

/// 優先順位に従って Bruker データセットのタイトル（実験名）を決定する
/// 1. pdata/1/title の中身（先頭の非空行）
/// 2. audita.txt から抽出したデータセット名
/// 3. acqus の TITLE パラメータ（定型文でない場合）
/// 4. フォルダ名（フォールバック）
pub fn resolve_title(
    title_content: Option<&str>,
    audita_content: Option<&str>,
    acqus: &HashMap<String, JcampValue>,
    fallback_name: &str,
) -> String {
    // 1. pdata/1/title
    if let Some(tc) = title_content {
        for line in tc.lines() {
            let t = line.trim();
            if !t.is_empty() && !is_generic_bruker_title(t) {
                return t.to_string();
            }
        }
    }

    // 2. audita.txt
    if let Some(ac) = audita_content {
        if let Some(audita_title) = extract_title_from_audita(ac) {
            if !is_generic_bruker_title(&audita_title) {
                return audita_title;
            }
        }
    }

    // 3. acqus TITLE
    if let Some(t) = acqus.get("TITLE").and_then(|v| v.as_str()) {
        let t = t.trim();
        if !is_generic_bruker_title(t) {
            return t.to_string();
        }
    }

    // 4. フォールバック (フォルダ名等)
    if !fallback_name.is_empty() {
        fallback_name.to_string()
    } else {
        "Bruker NMR".to_string()
    }
}

/// Bruker データの読み込みリーダー
pub struct BrukerReader;

impl BrukerReader {
    /// Bruker データ (ディレクトリまたはファイル) から生FIDデータを読み込む
    pub fn read_fid<P: AsRef<Path>>(path: P) -> Result<RawFid> {
        <Self as NmrDataSource>::read_fid(path)
    }

    /// JCAMP-DX ファイルからパラメータを読み込む
    pub fn read_acqus<P: AsRef<Path>>(path: P) -> Result<HashMap<String, JcampValue>> {

        let content = fs::read_to_string(path)?;
        Ok(parse_jcamp(&content))
    }

    /// fid バイナリファイルを読み込み、複素数配列を構築する
    pub fn read_fid_binary(
        fid_path: &Path,
        acqus: &HashMap<String, JcampValue>,
    ) -> Result<Vec<Complex64>> {
        let mut file = File::open(fid_path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        let bytorda = acqus
            .get("BYTORDA")
            .and_then(|v| v.as_u32())
            .unwrap_or(0);
        let is_big_endian = bytorda == 1;

        let dtypa = acqus
            .get("DTYPA")
            .and_then(|v| v.as_u32())
            .unwrap_or(0);

        let mut raw_numbers = Vec::new();

        if dtypa == 2 {
            // 64-bit float
            let point_bytes = 8;
            let n_vals = buffer.len() / point_bytes;
            raw_numbers.reserve(n_vals);
            for i in 0..n_vals {
                let chunk = &buffer[i * point_bytes..(i + 1) * point_bytes];
                let val = if is_big_endian {
                    BigEndian::read_f64(chunk)
                } else {
                    LittleEndian::read_f64(chunk)
                };
                raw_numbers.push(val);
            }
        } else {
            // 32-bit int (DTYPA = 0)
            let point_bytes = 4;
            let n_vals = buffer.len() / point_bytes;
            raw_numbers.reserve(n_vals);
            for i in 0..n_vals {
                let chunk = &buffer[i * point_bytes..(i + 1) * point_bytes];
                let val = if is_big_endian {
                    BigEndian::read_i32(chunk) as f64
                } else {
                    LittleEndian::read_i32(chunk) as f64
                };
                raw_numbers.push(val);
            }
        }

        let num_points = raw_numbers.len() / 2;
        if num_points == 0 {
            return Err(ResonaError::ParseError(
                "Bruker fid contains 0 complex points".to_string(),
            ));
        }

        // インターリーブ形式 [re0, im0, re1, im1, ...] から複素数配列を構築
        let mut fid = Vec::with_capacity(num_points);
        for i in 0..num_points {
            let re = raw_numbers[2 * i];
            let im = raw_numbers[2 * i + 1];
            fid.push(Complex64::new(re, im));
        }

        // DC オフセットの除去 (末尾 1000 点または 5% の平均を減算)
        let m = num_points.min(1000);
        let tail_sum: Complex64 = fid[num_points - m..num_points].iter().sum();
        let dc = tail_sum / (m as f64);
        for val in &mut fid {
            *val -= dc;
        }

        Ok(fid)
    }

    /// パラメータとファイル情報から AcquisitionMetadata を生成する
    pub fn build_metadata(
        acqus: &HashMap<String, JcampValue>,
        title_content: Option<&str>,
        audita_content: Option<&str>,
        num_points: usize,
        group_delay: Option<f64>,
        fallback_name: &str,
    ) -> AcquisitionMetadata {
        let title = resolve_title(title_content, audita_content, acqus, fallback_name);

        let date_time = if let Some(d) = acqus.get("DATE").and_then(|v| v.as_f64()) {
            format_unix_timestamp(d)
        } else {
            String::new()
        };

        let experiment = acqus
            .get("PULPROG")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let solvent = acqus
            .get("SOLVENT")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let nucleus = acqus
            .get("NUC1")
            .and_then(|v| v.as_str())
            .unwrap_or("1H")
            .to_string();

        let obs_freq_mhz = acqus
            .get("SFO1")
            .and_then(|v| v.as_f64())
            .unwrap_or(400.0);

        let spectral_width_hz = acqus
            .get("SW_H")
            .or_else(|| acqus.get("SW"))
            .and_then(|v| v.as_f64())
            .unwrap_or(8000.0);

        let scans = acqus
            .get("NS")
            .and_then(|v| v.as_u32())
            .unwrap_or(1);

        let acquisition_time_sec = if spectral_width_hz > 0.0 {
            num_points as f64 / spectral_width_hz
        } else {
            0.0
        };

        let relaxation_delay_sec = acqus
            .get("D")
            .and_then(|v| v.get_f64_at(1))
            .unwrap_or(0.0);

        let pulse_width_us = acqus.get("P").and_then(|v| v.get_f64_at(1));

        let pulse_power_attenuation_db = acqus
            .get("PL")
            .and_then(|v| v.get_f64_at(1))
            .or_else(|| acqus.get("PLW").and_then(|v| v.get_f64_at(1)));

        let pulse_angle_deg = if experiment.contains("30") {
            30.0
        } else if experiment.contains("90") || experiment.starts_with("zg") {
            90.0
        } else {
            90.0
        };

        let nuc2 = acqus
            .get("NUC2")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let cpdprg2 = acqus
            .get("CPDPRG")
            .and_then(|v| v.get_str_at(2))
            .unwrap_or("");

        let (decoupling, decoupling_nucleus, decoupling_sequence) =
            if !nuc2.is_empty() && !nuc2.eq_ignore_ascii_case("off") {
                let seq = if !cpdprg2.is_empty() {
                    cpdprg2.to_string()
                } else {
                    String::new()
                };
                ("TRUE".to_string(), nuc2.to_string(), seq)
            } else {
                (String::new(), String::new(), String::new())
            };

        // 温度 (K -> ℃)
        let temperature_celsius = if let Some(te) = acqus.get("TE").and_then(|v| v.as_f64()) {
            if te > 100.0 {
                te - 273.15
            } else {
                te
            }
        } else {
            25.0
        };

        let spin_rate_hz = acqus.get("RO").and_then(|v| v.as_f64());

        let origin = acqus
            .get("ORIGIN")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let title_header = acqus
            .get("TITLE")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let instrument = if title_header.contains("TopSpin") {
            if let Some(pos) = title_header.find("TopSpin") {
                format!("Bruker {}", &title_header[pos..].trim())
            } else {
                "Bruker BioSpin".to_string()
            }
        } else if !origin.is_empty() {
            origin.to_string()
        } else {
            "Bruker unknown".to_string()
        };

        let probe = String::new();


        let bf1 = acqus.get("BF1").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let o1 = acqus.get("O1").and_then(|v| v.as_f64()).unwrap_or(0.0);

        let center_ppm = if bf1 > 0.0 {
            o1 / bf1
        } else if obs_freq_mhz > 0.0 {
            o1 / obs_freq_mhz
        } else {
            0.0
        };

        AcquisitionMetadata {
            title,
            date_time,
            experiment,
            solvent,
            nucleus,
            obs_freq_mhz,
            spectral_width_hz,
            points: num_points,
            scans,
            acquisition_time_sec,
            relaxation_delay_sec,
            pulse_angle_deg,
            pulse_width_us,
            pulse_power_attenuation_db,
            pulse_shape: "RECT".to_string(),
            decoupling,
            decoupling_nucleus,
            decoupling_sequence,
            temperature_celsius,
            spin_rate_hz,
            instrument,
            probe,
            digital_filter_delay: group_delay,
            center_ppm,
        }
    }
}

impl NmrDataSource for BrukerReader {
    fn read_fid<P: AsRef<Path>>(path: P) -> Result<RawFid> {
        let p = path.as_ref();
        let resolved = resolve_bruker_paths(p)?;

        let acqus = Self::read_acqus(&resolved.acqus_path)?;

        let fid_data = Self::read_fid_binary(&resolved.fid_path, &acqus)?;
        let num_points = fid_data.len();

        let title_content = resolved
            .title_path
            .as_ref()
            .and_then(|tp| fs::read_to_string(tp).ok());
        let audita_content = resolved
            .audita_path
            .as_ref()
            .and_then(|ap| fs::read_to_string(ap).ok());

        let fallback_name = fallback_dataset_name(p);

        let grpdly = acqus.get("GRPDLY").and_then(|v| v.as_f64());
        let dspfvs = acqus.get("DSPFVS").and_then(|v| v.as_u32());
        let decim = acqus.get("DECIM").and_then(|v| v.as_u32());
        let group_delay = compute_bruker_group_delay(grpdly, dspfvs, decim);

        let metadata = Self::build_metadata(
            &acqus,
            title_content.as_deref(),
            audita_content.as_deref(),
            num_points,
            group_delay,
            &fallback_name,
        );

        Ok(RawFid {
            data: Array1::from_vec(fid_data),
            metadata,
            group_delay,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_generic_bruker_title() {
        assert!(is_generic_bruker_title("Parameter file, TopSpin 3.7.0"));
        assert!(is_generic_bruker_title("parameter file"));
        assert!(is_generic_bruker_title("Audit trail, TopSpin 3.7.0"));
        assert!(is_generic_bruker_title("Bruker BioSpin GmbH"));
        assert!(is_generic_bruker_title("TopSpin 3.7.0"));
        assert!(is_generic_bruker_title("topspin"));
        assert!(is_generic_bruker_title(""));
        assert!(is_generic_bruker_title("   "));
        assert!(is_generic_bruker_title("untitled"));

        assert!(!is_generic_bruker_title("PMP-vinyl-Indoline-sub-SI"));
        assert!(!is_generic_bruker_title("Proton 1D Spectrum"));
        assert!(!is_generic_bruker_title("Sample A in CDCl3"));
    }

    #[test]
    fn test_extract_title_from_audita() {
        let audita_unix = "##TITLE= Audit trail, TopSpin 3.7.0\n##JCAMPDX= 5.01\n##ORIGIN= Bruker BioSpin GmbH\n$$ /opt/data/thosjl/PMP-vinyl-Indoline-sub-SI/13/audita.txt\n##AUDIT TRAIL=";
        assert_eq!(
            extract_title_from_audita(audita_unix),
            Some("PMP-vinyl-Indoline-sub-SI".to_string())
        );

        let audita_win = "##TITLE= Audit trail\n$$ C:\\data\\user\\TestExp\\1\\audita.txt\n";
        assert_eq!(
            extract_title_from_audita(audita_win),
            Some("TestExp".to_string())
        );

        let audita_no_expno = "$$ /opt/nmr_data/Compound_42/audita.txt\n";
        assert_eq!(
            extract_title_from_audita(audita_no_expno),
            Some("Compound_42".to_string())
        );

        let audita_none = "##TITLE= Audit trail\n##OWNER= auto\n";
        assert_eq!(extract_title_from_audita(audita_none), None);
    }

    #[test]
    fn test_fallback_dataset_name() {
        assert_eq!(fallback_dataset_name(Path::new("/data/sample1/1")), "sample1");
        assert_eq!(fallback_dataset_name(Path::new("/data/sample1/1/fid")), "sample1");
        assert_eq!(fallback_dataset_name(Path::new("/data/Bruker_1H")), "Bruker_1H");
        assert_eq!(fallback_dataset_name(Path::new("Bruker_1H")), "Bruker_1H");
    }

    #[test]
    fn test_resolve_title_priority() {
        let mut acqus = HashMap::new();
        acqus.insert(
            "TITLE".to_string(),
            JcampValue::Text("Parameter file, TopSpin 3.7.0".to_string()),
        );

        let audita = "$$ /opt/data/user/MyDataset/10/audita.txt";

        // 1. pdata/1/title が最優先
        let title_content = "Real Experiment Title\nLine 2";
        let res1 = resolve_title(Some(title_content), Some(audita), &acqus, "FallbackDir");
        assert_eq!(res1, "Real Experiment Title");

        // 2. pdata/1/title が定型文や空なら audita.txt
        let res2 = resolve_title(Some("Parameter file"), Some(audita), &acqus, "FallbackDir");
        assert_eq!(res2, "MyDataset");

        let res2_none = resolve_title(None, Some(audita), &acqus, "FallbackDir");
        assert_eq!(res2_none, "MyDataset");

        // 3. audita もないが acqus に本物のタイトルが入っている場合
        let mut acqus_real = HashMap::new();
        acqus_real.insert(
            "TITLE".to_string(),
            JcampValue::Text("Custom Experiment Name".to_string()),
        );
        let res3 = resolve_title(None, None, &acqus_real, "FallbackDir");
        assert_eq!(res3, "Custom Experiment Name");

        // 4. 全部定型文ならフォルダ名フォールバック
        let res4 = resolve_title(None, None, &acqus, "FallbackDir");
        assert_eq!(res4, "FallbackDir");
    }
}

