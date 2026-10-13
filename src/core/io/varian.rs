use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use byteorder::{BigEndian, ByteOrder};
use ndarray::Array1;
use num_complex::Complex64;

use crate::core::error::{ResonaError, Result};
use crate::core::io::traits::{AcquisitionMetadata, NmrDataSource, RawFid};

/// Varian procpar パラメータ値の表現
#[derive(Debug, Clone, PartialEq)]
pub enum ProcparValue {
    Real(Vec<f64>),
    String(Vec<String>),
}

impl ProcparValue {
    pub fn as_f64(&self) -> Option<f64> {
        self.get_f64_at(0)
    }

    pub fn as_u32(&self) -> Option<u32> {
        self.as_f64().map(|n| n.round() as u32)
    }

    pub fn as_usize(&self) -> Option<usize> {
        self.as_f64().map(|n| n.round() as usize)
    }

    pub fn as_str(&self) -> Option<&str> {
        self.get_str_at(0)
    }

    pub fn get_f64_at(&self, idx: usize) -> Option<f64> {
        match self {
            ProcparValue::Real(vec) => vec.get(idx).copied(),
            ProcparValue::String(vec) => vec.get(idx).and_then(|s| s.parse::<f64>().ok()),
        }
    }

    pub fn get_str_at(&self, idx: usize) -> Option<&str> {
        match self {
            ProcparValue::String(vec) => vec.get(idx).map(|s| s.as_str()),
            ProcparValue::Real(_vec) => None,
        }
    }
}

/// Varian procpar テキストをパースしてキーと値のマップを生成
pub fn parse_procpar(content: &str) -> HashMap<String, ProcparValue> {
    let mut params = HashMap::new();
    let mut lines = content.lines().peekable();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }

        let name = parts[0].to_string();
        let basictype = parts[2]; // "1": real, "2": string

        // 2行目: 値の個数と値
        let val_line = match lines.next() {
            Some(l) => l.trim(),
            None => break,
        };

        if basictype == "1" {
            // 数値: 1行に `count val1 val2 ...`
            let val_parts: Vec<&str> = val_line.split_whitespace().collect();
            if !val_parts.is_empty() {
                let count: usize = val_parts[0].parse().unwrap_or(0);
                let mut reals = Vec::with_capacity(count);
                for token in val_parts.iter().skip(1).take(count) {
                    if let Ok(v) = token.parse::<f64>() {
                        reals.push(v);
                    }
                }
                params.insert(name, ProcparValue::Real(reals));
            }
        } else if basictype == "2" {
            // 文字列: `count "val1"`。count > 1 の場合は複数行に続く場合あり
            let val_parts: Vec<&str> = val_line.split_whitespace().collect();
            let count: usize = val_parts
                .first()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);

            let mut strings = Vec::with_capacity(count);

            // 1行目の文字列をダブルクォートから抽出
            if let Some(first_quote) = val_line.find('"') {
                if let Some(last_quote) = val_line.rfind('"') {
                    if last_quote > first_quote {
                        strings.push(val_line[first_quote + 1..last_quote].to_string());
                    } else {
                        strings.push(val_line[first_quote + 1..].to_string());
                    }
                }
            }

            // 2つ目以降の文字列行（存在する場合）
            while strings.len() < count {
                if let Some(next_line) = lines.next() {
                    let next_trimmed = next_line.trim();
                    if let Some(first_quote) = next_trimmed.find('"') {
                        if let Some(last_quote) = next_trimmed.rfind('"') {
                            if last_quote > first_quote {
                                strings.push(next_trimmed[first_quote + 1..last_quote].to_string());
                            } else {
                                strings.push(next_trimmed[first_quote + 1..].to_string());
                            }
                        }
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            params.insert(name, ProcparValue::String(strings));
        }

        // 3行目: enumerable (読み飛ばす)
        let _ = lines.next();
    }

    params
}

/// 32-byte の Varian fid ファイルヘッダー
#[derive(Debug, Clone, Copy)]
pub struct VarianFileHeader {
    pub nblocks: i32,
    pub ntraces: i32,
    pub np: i32,
    pub ebytes: i32,
    pub tbytes: i32,
    pub bbytes: i32,
    pub vers_id: i16,
    pub status: i16,
    pub nbheaders: i32,
}

impl VarianFileHeader {
    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        Self {
            nblocks: BigEndian::read_i32(&bytes[0..4]),
            ntraces: BigEndian::read_i32(&bytes[4..8]),
            np: BigEndian::read_i32(&bytes[8..12]),
            ebytes: BigEndian::read_i32(&bytes[12..16]),
            tbytes: BigEndian::read_i32(&bytes[16..20]),
            bbytes: BigEndian::read_i32(&bytes[20..24]),
            vers_id: BigEndian::read_i16(&bytes[24..26]),
            status: BigEndian::read_i16(&bytes[26..28]),
            nbheaders: BigEndian::read_i32(&bytes[28..32]),
        }
    }
}

/// Varian データから解決されたパス群
#[derive(Debug, Clone, PartialEq)]
pub struct VarianResolvedPaths {
    pub fid_path: PathBuf,
    pub procpar_path: PathBuf,
    pub text_path: Option<PathBuf>,
}

/// ディレクトリやファイルから Varian データのパス群を解決する
pub fn resolve_varian_paths(path: &Path) -> Result<VarianResolvedPaths> {
    if path.is_dir() {
        let fid_path = path.join("fid");
        let procpar_path = path.join("procpar");
        if fid_path.is_file() && procpar_path.is_file() {
            let text_path = path.join("text");
            let text_opt = if text_path.is_file() {
                Some(text_path)
            } else {
                None
            };
            return Ok(VarianResolvedPaths {
                fid_path,
                procpar_path,
                text_path: text_opt,
            });
        }

        Err(ResonaError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "Varian fid or procpar not found in directory {}",
                path.display()
            ),
        )))
    } else {
        let file_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        let parent = path.parent().unwrap_or_else(|| Path::new("."));

        let (fid_path, procpar_path) = if file_name == "fid" {
            let procpar = parent.join("procpar");
            if !procpar.is_file() {
                return Err(ResonaError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("procpar not found in {}", parent.display()),
                )));
            }
            (path.to_path_buf(), procpar)
        } else if file_name == "procpar" {
            let fid = parent.join("fid");
            if !fid.is_file() {
                return Err(ResonaError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("fid not found in {}", parent.display()),
                )));
            }
            (fid, path.to_path_buf())
        } else {
            return Err(ResonaError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Not a Varian data file: {}", path.display()),
            )));
        };

        let text_path = parent.join("text");
        let text_opt = if text_path.is_file() {
            Some(text_path)
        } else {
            None
        };

        Ok(VarianResolvedPaths {
            fid_path,
            procpar_path,
            text_path: text_opt,
        })
    }
}

/// 核種表記を標準化 ("H1" -> "1H", "C13" -> "13C" 等)
fn normalize_nucleus(raw: &str) -> String {
    let s = raw.trim();
    if s.is_empty() {
        return "1H".to_string();
    }
    // "H1" -> "1H", "C13" -> "13C", "P31" -> "31P", "F19" -> "19F"
    let mut letters = String::new();
    let mut digits = String::new();
    for c in s.chars() {
        if c.is_ascii_alphabetic() {
            letters.push(c);
        } else if c.is_ascii_digit() {
            digits.push(c);
        }
    }
    if !digits.is_empty() && !letters.is_empty() {
        format!("{}{}", digits, letters.to_uppercase())
    } else {
        s.to_string()
    }
}

/// time_run ("20261009T184404") をフォーマット ("2026-10-09 18:44:04")
fn format_time_run(time_run: &str) -> String {
    let tr = time_run.trim();
    if tr.len() >= 15 && tr.contains('T') {
        let parts: Vec<&str> = tr.split('T').collect();
        if parts.len() == 2 && parts[0].len() == 8 && parts[1].len() >= 6 {
            let y = &parts[0][0..4];
            let m = &parts[0][4..6];
            let d = &parts[0][6..8];
            let h = &parts[1][0..2];
            let min = &parts[1][2..4];
            let sec = &parts[1][4..6];
            return format!("{}-{}-{} {}:{}:{}", y, m, d, h, min, sec);
        }
    }
    tr.to_string()
}

/// Varian データリーダー
pub struct VarianReader;

impl VarianReader {
    pub fn read_fid<P: AsRef<Path>>(path: P) -> Result<RawFid> {
        <Self as NmrDataSource>::read_fid(path)
    }

    /// fid バイナリファイルを読み込み、複素数配列を構築する
    pub fn read_fid_binary(
        fid_path: &Path,
        header: &VarianFileHeader,
    ) -> Result<Vec<Complex64>> {
        let mut file = File::open(fid_path)?;

        // ファイルヘッダー (32 bytes) をスキップ
        file.seek(SeekFrom::Start(32))?;

        // 各ブロックの先頭には nbheaders * 28 バイトのブロックヘッダーが存在
        let block_header_bytes = (header.nbheaders.max(0) as usize) * 28;
        if block_header_bytes > 0 {
            file.seek(SeekFrom::Current(block_header_bytes as i64))?;
        }

        let np = header.np as usize;
        if np == 0 {
            return Err(ResonaError::ParseError(
                "Varian fid contains 0 points".to_string(),
            ));
        }

        let is_float = (header.status & 0x8) != 0;
        let is_int32 = (header.status & 0x4) != 0;

        let mut raw_numbers = Vec::with_capacity(np);

        if is_float {
            // 32-bit float (Big-Endian)
            let mut buf = vec![0u8; np * 4];
            file.read_exact(&mut buf)?;
            for i in 0..np {
                let chunk = &buf[i * 4..(i + 1) * 4];
                let val = BigEndian::read_f32(chunk) as f64;
                raw_numbers.push(val);
            }
        } else if is_int32 {
            // 32-bit int (Big-Endian)
            let mut buf = vec![0u8; np * 4];
            file.read_exact(&mut buf)?;
            for i in 0..np {
                let chunk = &buf[i * 4..(i + 1) * 4];
                let val = BigEndian::read_i32(chunk) as f64;
                raw_numbers.push(val);
            }
        } else {
            // 16-bit int (Big-Endian)
            let mut buf = vec![0u8; np * 2];
            file.read_exact(&mut buf)?;
            for i in 0..np {
                let chunk = &buf[i * 2..(i + 1) * 2];
                let val = BigEndian::read_i16(chunk) as f64;
                raw_numbers.push(val);
            }
        }

        let num_complex_points = raw_numbers.len() / 2;
        if num_complex_points == 0 {
            return Err(ResonaError::ParseError(
                "Varian fid contains 0 complex points".to_string(),
            ));
        }

        // インターリーブ形式 [re0, im0, re1, im1, ...] から複素数配列を構築
        // Varian は虚部 (Quadrature channel) の位相極性が標準形式と逆であるため、符号反転 (nmrglue の imagfactor: -1.0 相当) を行う
        let mut fid = Vec::with_capacity(num_complex_points);
        for i in 0..num_complex_points {
            let re = raw_numbers[2 * i];
            let im = raw_numbers[2 * i + 1];
            fid.push(Complex64::new(re, -im));
        }

        // DC オフセットの除去 (末尾 1000 点または 5% の平均を減算)
        let m = num_complex_points.min(1000);
        let tail_sum: Complex64 = fid[num_complex_points - m..num_complex_points]
            .iter()
            .sum();
        let dc = tail_sum / (m as f64);
        for val in &mut fid {
            *val -= dc;
        }

        Ok(fid)
    }

    /// パラメータとメタデータから AcquisitionMetadata を生成する
    pub fn build_metadata(
        procpar: &HashMap<String, ProcparValue>,
        text_content: Option<&str>,
        fallback_name: &str,
    ) -> AcquisitionMetadata {
        // Title
        let title = if let Some(tc) = text_content {
            let first_line = tc.lines().map(|s| s.trim()).find(|s| !s.is_empty());
            if let Some(tl) = first_line {
                tl.to_string()
            } else if let Some(f) = procpar.get("file").and_then(|v| v.as_str()) {
                f.trim().to_string()
            } else {
                fallback_name.to_string()
            }
        } else if let Some(f) = procpar.get("file").and_then(|v| v.as_str()) {
            let f_trimmed = f.trim();
            if !f_trimmed.is_empty() {
                f_trimmed.to_string()
            } else {
                fallback_name.to_string()
            }
        } else {
            fallback_name.to_string()
        };

        // Date / Time
        let date_time = if let Some(tr) = procpar.get("time_run").and_then(|v| v.as_str()) {
            format_time_run(tr)
        } else if let Some(d) = procpar.get("date").and_then(|v| v.as_str()) {
            d.trim().to_string()
        } else {
            String::new()
        };

        // Nucleus
        let nucleus_raw = procpar
            .get("tn")
            .and_then(|v| v.as_str())
            .unwrap_or("1H");
        let nucleus = normalize_nucleus(nucleus_raw);

        // Experiment (seqfil)
        let experiment = procpar
            .get("seqfil")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Frequency & Spectral Width
        let obs_freq_mhz = procpar
            .get("sfrq")
            .and_then(|v| v.as_f64())
            .unwrap_or(400.0);

        let spectral_width_hz = procpar
            .get("sw")
            .and_then(|v| v.as_f64())
            .unwrap_or(8000.0);

        let np = procpar.get("np").and_then(|v| v.as_usize()).unwrap_or(0);
        let points = np / 2;

        let scans = procpar
            .get("ct")
            .and_then(|v| v.as_u32())
            .or_else(|| procpar.get("nt").and_then(|v| v.as_u32()))
            .unwrap_or(1);

        let acquisition_time_sec = procpar
            .get("at")
            .and_then(|v| v.as_f64())
            .unwrap_or_else(|| {
                if spectral_width_hz > 0.0 {
                    points as f64 / spectral_width_hz
                } else {
                    0.0
                }
            });

        let relaxation_delay_sec = procpar
            .get("d1")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let pulse_width_us = procpar.get("pw").and_then(|v| v.as_f64());
        let pw90 = procpar.get("pw90").and_then(|v| v.as_f64());
        let pulse_angle_deg = if let (Some(pw), Some(pw90_val)) = (pulse_width_us, pw90) {
            if pw90_val > 0.0 {
                (pw / pw90_val * 90.0).clamp(0.0, 360.0)
            } else {
                90.0
            }
        } else {
            90.0
        };

        let pulse_power_attenuation_db = procpar.get("tpwr").and_then(|v| v.as_f64());

        // Decoupling
        let dn_raw = procpar.get("dn").and_then(|v| v.as_str()).unwrap_or("");
        let dm = procpar.get("dm").and_then(|v| v.as_str()).unwrap_or("");
        let is_decoupled = dm.contains('y') || dm.contains('Y');

        let (decoupling, decoupling_nucleus, decoupling_sequence) = if is_decoupled && !dn_raw.is_empty() {
            let dseq = procpar
                .get("dseq")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            ("TRUE".to_string(), normalize_nucleus(dn_raw), dseq)
        } else {
            (String::new(), String::new(), String::new())
        };

        // Temperature (Varian の temp はすでに ℃)
        let temperature_celsius = procpar
            .get("temp")
            .and_then(|v| v.as_f64())
            .unwrap_or(25.0);

        let spin_rate_hz = procpar.get("spin").and_then(|v| v.as_f64());

        // Instrument
        let console = procpar
            .get("console")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let system_name = procpar
            .get("system")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let instrument = if !console.is_empty() {
            format!("Varian {}", console)
        } else if !system_name.is_empty() {
            format!("Varian {}", system_name)
        } else {
            "Varian NMR".to_string()
        };

        let probe = procpar
            .get("probe_")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let solvent = procpar
            .get("solvent")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // 観測中心 PPM
        let rfl = procpar.get("rfl").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let rfp = procpar.get("rfp").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let center_ppm = if obs_freq_mhz > 0.0 {
            rfp + (spectral_width_hz - 2.0 * rfl) / (2.0 * obs_freq_mhz)
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
            points,
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
            digital_filter_delay: None,
            center_ppm,
        }
    }
}

impl NmrDataSource for VarianReader {
    fn read_fid<P: AsRef<Path>>(path: P) -> Result<RawFid> {
        let p = path.as_ref();
        let resolved = resolve_varian_paths(p)?;

        // procpar パース
        let procpar_content = fs::read_to_string(&resolved.procpar_path)?;
        let procpar = parse_procpar(&procpar_content);

        // text ファイル（存在すれば読み取り）
        let text_content = if let Some(ref tp) = resolved.text_path {
            fs::read_to_string(tp).ok()
        } else {
            None
        };

        // fid ファイルヘッダー (32 bytes) 読み取り
        let mut fid_file = File::open(&resolved.fid_path)?;
        let mut header_buf = [0u8; 32];
        fid_file.read_exact(&mut header_buf)?;
        let file_header = VarianFileHeader::from_bytes(&header_buf);

        // バイナリデータ読み取り
        let fid_data = Self::read_fid_binary(&resolved.fid_path, &file_header)?;

        // フォールバックデータセット名
        let parent_dir = resolved
            .fid_path
            .parent()
            .unwrap_or(&resolved.fid_path);
        let folder_name = parent_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("Varian NMR");
        let fallback_name = folder_name
            .strip_suffix(".fid")
            .unwrap_or(folder_name);

        let metadata = Self::build_metadata(&procpar, text_content.as_deref(), fallback_name);

        Ok(RawFid {
            data: Array1::from_vec(fid_data),
            metadata,
            group_delay: None,
        })
    }
}
