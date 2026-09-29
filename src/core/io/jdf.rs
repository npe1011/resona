use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use byteorder::{BigEndian, ByteOrder, LittleEndian};
use ndarray::Array1;
use num_complex::Complex64;

use crate::core::error::{ResonaError, Result};
use crate::core::io::traits::{AcquisitionMetadata, NmrDataSource, RawFid};

pub struct JeolJdfReader;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Endianness {
    Big,
    Little,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DataType {
    Float64,
    Float32,
    #[allow(dead_code)]
    Int32,
    #[allow(dead_code)]
    Int16,
}

impl NmrDataSource for JeolJdfReader {
    fn read_fid<P: AsRef<Path>>(path: P) -> Result<RawFid> {
        let mut file = File::open(path)?;
        let mut header_buf = [0u8; 1296];
        file.read_exact(&mut header_buf)?;

        // 1. ファイル識別子の確認 (JEOL.NMR または JEOL.DAT)
        if !header_buf[0..8].starts_with(b"JEOL.") {
            return Err(ResonaError::InvalidJdfHeader(format!(
                "Invalid file identifier: {:?}",
                String::from_utf8_lossy(&header_buf[0..8])
            )));
        }

        // 2. エンディアンの判定 (byte 8)
        let endian = match header_buf[8] {
            0 => Endianness::Big,
            1 => Endianness::Little,
            other => {
                return Err(ResonaError::InvalidJdfHeader(format!(
                    "Unknown endian value: {}",
                    other
                )));
            }
        };

        // 3. データ形式・単位チェック
        // units[0]: byte 32. 0: none, 1: sec, 2: hz, 3: ppm
        let unit_0 = header_buf[32];
        if unit_0 == 2 || unit_0 == 3 {
            return Err(ResonaError::ProcessedDataNotSupported(
                "Input file is already processed in frequency domain (Hz/ppm). Only time-domain FID files are supported.".to_string(),
            ));
        }

        // data_type (byte 14)
        // nmrglue ConversionTable: 0: float64, 1: float32
        let data_type_byte = header_buf[14];
        let data_type = match data_type_byte >> 6 {
            0 => DataType::Float64,
            1 => DataType::Float32,
            other => {
                return Err(ResonaError::InvalidJdfHeader(format!(
                    "Unknown data_type: {}",
                    other
                )));
            }
        };

        // title (byte 40..164, 124 bytes)
        let title_raw = &header_buf[40..164];
        let title = String::from_utf8_lossy(title_raw)
            .trim_matches('\0')
            .trim()
            .to_string();

        // data_points (byte 176..180, BigEndian uint32)
        let num_points = BigEndian::read_u32(&header_buf[176..180]) as usize;

        // param_start (byte 1212..1216), param_length (byte 1216..1220) - Always BigEndian
        let param_start = BigEndian::read_u32(&header_buf[1212..1216]) as u64;
        let _param_length = BigEndian::read_u32(&header_buf[1216..1220]) as usize;

        // data_start (byte 1284..1288) - Always BigEndian
        let data_start = BigEndian::read_u32(&header_buf[1284..1288]) as u64;

        // 4. パラメータブロックの読み込み
        // param_start から先頭16バイト: psize(4), lidx(4), hidx(4), tsize(4)
        file.seek(SeekFrom::Start(param_start))?;
        let mut param_hdr = [0u8; 16];
        file.read_exact(&mut param_hdr)?;

        let num_params = match endian {
            Endianness::Big => BigEndian::read_u32(&param_hdr[8..12]) as usize,
            Endianness::Little => LittleEndian::read_u32(&param_hdr[8..12]) as usize,
        };

        let mut param_data = vec![0u8; num_params * 64];
        file.read_exact(&mut param_data)?;
        let mut orders_str = String::new();
        let mut factors_str = String::new();

        let mut obs_freq_mhz = 0.0;
        let mut spectral_width_hz = 0.0;
        let mut center_ppm = 0.0;
        let mut nucleus = "1H".to_string();
        let mut solvent = String::new();
        let mut scans = 1u32;
        let mut acq_time_sec = 0.0;
        let mut relaxation_delay_sec = 0.0;
        let mut pulse_angle_deg = 0.0;
        let mut pulse_width_us: Option<f64> = None;
        let mut pulse_power_attenuation_db: Option<f64> = None;
        let mut pulse_shape = String::new();
        let mut decoupling = String::new();
        let mut decoupling_nucleus = String::new();
        let mut decoupling_sequence = String::new();
        let mut temperature_celsius = 25.0;
        let mut spin_rate_hz: Option<f64> = None;
        let mut instrument = String::new();
        let mut experiment = String::new();
        let mut actual_start_time: Option<f64> = None;




        for i in 0..num_params {
            let offset = i * 64;
            let block = &param_data[offset..offset + 64];

            // 0x04..0x06: unit_scaler (int16)
            let scaler = match endian {
                Endianness::Big => BigEndian::read_i16(&block[4..6]),
                Endianness::Little => LittleEndian::read_i16(&block[4..6]),
            };
            let scale_factor = 10f64.powi(scaler as i32);

            // 0x20..0x24: value_type (uint32)
            let val_type = match endian {
                Endianness::Big => BigEndian::read_u32(&block[32..36]),
                Endianness::Little => LittleEndian::read_i32(&block[32..36]) as u32,
            };

            // 0x24..0x40 (28 bytes): parameter name (ASCII)
            let name_raw = &block[36..64];
            let name = String::from_utf8_lossy(name_raw)
                .trim_matches('\0')
                .trim()
                .to_lowercase();

            if val_type == 0 {
                // String (0x10..0x20, 16 bytes)
                let val_str = String::from_utf8_lossy(&block[16..32])
                    .trim_matches('\0')
                    .trim()
                    .to_string();

                if name == "orders" {
                    orders_str = val_str;
                } else if name == "factors" {
                    factors_str = val_str;
                } else if name == "x_domain" {
                    nucleus = val_str;
                } else if name == "solvent" {
                    solvent = val_str;
                } else if name == "inst_model_number" || name == "instrument_model_number" || name == "instrument" {
                    if !val_str.is_empty() {
                        instrument = val_str;
                    }
                } else if name == "experiment" {
                    experiment = val_str;
                } else if name == "pulse_in_file" {
                    pulse_shape = val_str;
                } else if name == "decoupling" {
                    decoupling = val_str;
                } else if name == "irr_domain" {
                    decoupling_nucleus = val_str;
                } else if name == "irr_noise" {
                    decoupling_sequence = val_str;
                }
            } else {

                let numeric_val = match val_type {
                    1 => {
                        // Integer (0x10..0x14, 4 bytes)
                        let val_int = match endian {
                            Endianness::Big => BigEndian::read_i32(&block[16..20]),
                            Endianness::Little => LittleEndian::read_i32(&block[16..20]),
                        };
                        Some(val_int as f64 * scale_factor)
                    }
                    2 => {
                        // Float (0x10..0x18, 8 bytes double)
                        let val_f64 = match endian {
                            Endianness::Big => BigEndian::read_f64(&block[16..24]),
                            Endianness::Little => LittleEndian::read_f64(&block[16..24]),
                        };
                        Some(val_f64 * scale_factor)
                    }
                    _ => None,
                };

                if let Some(val) = numeric_val {
                    if name == "x_freq" {
                        obs_freq_mhz = val / 1e6;
                    } else if name == "x_sweep" {
                        spectral_width_hz = val;
                    } else if name == "x_offset" {
                        center_ppm = val;
                    } else if name == "scans" {
                        scans = val.round().max(1.0) as u32;
                    } else if name == "x_acq_time" || name == "x_acq_duration" {
                        acq_time_sec = val;
                    } else if name == "relaxation_delay" {
                        relaxation_delay_sec = val;
                    } else if name == "x_angle" {
                        pulse_angle_deg = val;
                    } else if name == "temp_get" {
                        temperature_celsius = val;
                    } else if name == "actual_start_time" {
                        actual_start_time = Some(val);
                    } else if name == "x_pulse" {
                        pulse_width_us = Some(val);
                    } else if name == "x_atn" || name == "xatn" {
                        pulse_power_attenuation_db = Some(val);
                    } else if name == "spin_get" {
                        spin_rate_hz = Some(val);
                    }
                }
            }
        }

        // 5. 機器名 (Instrument) の決定 (テキスト設定がなければ JEOL unknown)
        if instrument.is_empty() {
            instrument = "JEOL unknown".to_string();
        }
        let probe = String::new();

        // 6. デジタルフィルタ群遅延 (Group Delay) の計算
        let group_delay = compute_jeol_group_delay(&orders_str, &factors_str);



        // 6. 生データブロックのパース (1D: 2 sections: Real, Imag)
        file.seek(SeekFrom::Start(data_start))?;
        let total_samples = num_points * 2;
        let mut raw_floats = Vec::with_capacity(total_samples);

        match data_type {
            DataType::Float32 => {
                let mut buf = vec![0u8; total_samples * 4];
                file.read_exact(&mut buf)?;
                for chunk in buf.chunks_exact(4) {
                    let v = match endian {
                        Endianness::Big => BigEndian::read_f32(chunk),
                        Endianness::Little => LittleEndian::read_f32(chunk),
                    };
                    raw_floats.push(v as f64);
                }
            }
            DataType::Float64 => {
                let mut buf = vec![0u8; total_samples * 8];
                file.read_exact(&mut buf)?;
                for chunk in buf.chunks_exact(8) {
                    let v = match endian {
                        Endianness::Big => BigEndian::read_f64(chunk),
                        Endianness::Little => LittleEndian::read_f64(chunk),
                    };
                    raw_floats.push(v);
                }
            }
            DataType::Int32 => {
                let mut buf = vec![0u8; total_samples * 4];
                file.read_exact(&mut buf)?;
                for chunk in buf.chunks_exact(4) {
                    let v = match endian {
                        Endianness::Big => BigEndian::read_i32(chunk),
                        Endianness::Little => LittleEndian::read_i32(chunk),
                    };
                    raw_floats.push(v as f64);
                }
            }
            DataType::Int16 => {
                let mut buf = vec![0u8; total_samples * 2];
                file.read_exact(&mut buf)?;
                for chunk in buf.chunks_exact(2) {
                    let v = match endian {
                        Endianness::Big => BigEndian::read_i16(chunk),
                        Endianness::Little => LittleEndian::read_i16(chunk),
                    };
                    raw_floats.push(v as f64);
                }
            }
        }

        // 7. 複素数FIDの再構成: sections[0] - j * sections[1]
        let real_part = &raw_floats[0..num_points];
        let imag_part = &raw_floats[num_points..total_samples];

        let mut fid = Vec::with_capacity(num_points);
        for n in 0..num_points {
            fid.push(Complex64::new(real_part[n], -imag_part[n]));
        }

        // 8. DCオフセットの除去 (末尾1000点の平均を減算)
        let m = num_points.min(1000);
        let tail_sum: Complex64 = fid[num_points - m..num_points].iter().sum();
        let dc = tail_sum / (m as f64);

        for val in &mut fid {
            *val -= dc;
        }

        let date_time = if let Some(ts) = actual_start_time {
            format_jeol_timestamp(ts)
        } else {
            let info0 = header_buf[392];
            let info1 = header_buf[393];
            let info2 = header_buf[394];
            let c_year = 1990 + ((info0 >> 1) as i32);
            let c_month = (((info0 << 3) & 0b00001000) | (info1 >> 5)) as i32;
            let c_day = (info2 & 0b00011111) as i32;
            if c_year >= 1990 && c_month >= 1 && c_month <= 12 && c_day >= 1 && c_day <= 31 {
                format!("{:04}-{:02}-{:02}", c_year, c_month, c_day)
            } else {
                String::new()
            }
        };

        let metadata = AcquisitionMetadata {
            title,
            date_time,
            experiment,
            solvent,
            nucleus,
            obs_freq_mhz,
            spectral_width_hz,
            points: num_points,
            scans,
            acquisition_time_sec: acq_time_sec,
            relaxation_delay_sec,
            pulse_angle_deg,
            pulse_width_us,
            pulse_power_attenuation_db,
            pulse_shape,
            decoupling,
            decoupling_nucleus,
            decoupling_sequence,
            temperature_celsius,
            spin_rate_hz,
            instrument,
            probe,
            digital_filter_delay: group_delay,
            center_ppm,
        };

        Ok(RawFid {
            data: Array1::from_vec(fid),
            metadata,
            group_delay,
        })
    }
}

/// JEOLの orders, factors 文字列から Group Delay を計算
pub fn compute_jeol_group_delay(orders_str: &str, factors_str: &str) -> Option<f64> {
    if orders_str.is_empty() || factors_str.is_empty() {
        return None;
    }

    let orders: Vec<f64> = orders_str
        .split_whitespace()
        .filter_map(|s| s.parse::<f64>().ok())
        .collect();
    let factors: Vec<f64> = factors_str
        .split_whitespace()
        .filter_map(|s| s.parse::<f64>().ok())
        .collect();

    let k = factors.len();
    if k == 0 {
        return None;
    }

    // orders が factors より1つ多い場合は先頭要素を除外 (orders[1..])
    let orders_slice = if orders.len() == k + 1 {
        &orders[1..]
    } else if orders.len() == k {
        &orders[..]
    } else {
        return None;
    };

    // prod_fact_j = product of factors[j..k]
    let mut prod_fact = vec![1.0; k];
    let mut current_prod = 1.0;
    for j in (0..k).rev() {
        current_prod *= factors[j];
        prod_fact[j] = current_prod;
    }

    // D = 0.5 * sum_{i=0}^{k-1} (orders_slice[i] - 1) / prod_fact[i]
    let mut delay = 0.0;
    for i in 0..k {
        delay += (orders_slice[i] - 1.0) / prod_fact[i];
    }
    delay /= 2.0;

    Some(delay)
}

/// JEOLエポック (1990-01-01 00:00:00) からの経過秒数を "YYYY-MM-DD HH:MM:SS" 形式にフォーマット
pub fn format_jeol_timestamp(jeol_seconds: f64) -> String {
    let total_secs = jeol_seconds as i64;
    if total_secs <= 0 {
        return String::new();
    }

    let days = total_secs / 86400;
    let rem_secs = total_secs % 86400;
    let hour = rem_secs / 3600;
    let min = (rem_secs % 3600) / 60;
    let sec = rem_secs % 60;

    let mut y = 1990;
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
        31, if is_leap { 29 } else { 28 }, 31, 30, 31, 30,
        31, 31, 30, 31, 30, 31,
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
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, m, day, hour, min, sec)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_group_delay_formula() {
        // 例: orders="12 4 8", factors="2 4 8"
        let delay = compute_jeol_group_delay("12 4 8", "2 4 8");
        assert!(delay.is_some());
    }
}
