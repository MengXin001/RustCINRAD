use binread::prelude::*;
use core::f64;
use std::error::Error;
use std::io::Cursor;

use crate::io;
use crate::io::dtype::*;
use tracing::{error, info};

const CON: f64 = (180.0 / 4096.0) * 0.125;

#[allow(non_snake_case, unused_variables, unused_assignments)]
pub fn SAB_reader(path: &str) -> Result<StandardData, Box<dyn Error>> {
    let data = std::fs::read(path).unwrap_or_else(|e| {
        error!("failed to read file: {}", e);
        std::process::exit(1);
    });
    let site_code = io::base::infer_type(path).unwrap();
    let radarinfo = io::base::get_radar_info();
    let station = &radarinfo[site_code.clone()];
    let site_name = station[0].as_str().unwrap().to_string();
    let centerlon = station[1].as_f64().unwrap();
    let centerlat = station[2].as_f64().unwrap();
    let radartype = station[3].as_str().unwrap().to_string();
    let site_altitude = station[4].as_f64().unwrap();

    let radial_num = 2432;
    let count: usize = data.len() / radial_num; // SAB
    let mut dv: u16;
    let mut vcp_mode: String = "Unknown".to_string();
    let mut Rreso: f64 = 1.00;
    let mut Vreso: f64 = 0.25;

    let mut temp_data: Vec<(f64, Vec<f64>, Vec<f64>, Vec<f64>)> = Vec::with_capacity(count);
    let mut elevations = Vec::new();
    let mut REF = Vec::new();
    let mut VEL = Vec::new();
    let mut SW = Vec::new();
    let mut azimuths = Vec::new();

    for i in 0..count {
        let s = i * radial_num;
        let e = (i + 1) * radial_num;
        let mut cursor = Cursor::new(&data[s..e]);
        let sab_dtype: SAB_dtype = cursor.read_le()?;
        let s_info: S_INFO = sab_dtype.s_info;
        let sab_data: SAB_DATA = sab_dtype.sab_data;
        if i == 0 {
            dv = s_info.v_reso;
            Rreso = s_info.gate_length_r as f64 / 1000.0;
            Vreso = s_info.gate_length_v as f64 / 1000.0;
        }
        let elevation: f64 = s_info.elevation as f64 * CON;
        let azimuth: f64 = s_info.azimuth as f64 * CON;
        vcp_mode = s_info.vcp_mode.to_string();
        let v_reso: u16 = s_info.v_reso;
        let r_start = s + 128;
        let r_end = r_start + s_info.gate_num_r as usize;

        let r: Vec<f64> = data[r_start..r_end]
            .iter()
            .map(|&x| {
                let value = (x as f64 - 2.0) / 2.0 - 32.0;
                if value >= 0.0 {
                    value
                } else {
                    0.0
                }
            })
            .collect();

        let v_start = r_end;
        let v_end = v_start + s_info.gate_num_v as usize;
        let v: Vec<f64> = data[v_start..v_end]
            .iter()
            .map(|&x| {
                if x >= 2 {
                    if v_reso == 2 {
                        (x as f64 - 2.0) / 2.0 - 63.5
                    } else if v_reso == 4 {
                        (x as f64 - 2.0) - 127.0
                    } else {
                        f64::NAN
                    }
                } else if x == 1 {
                    f64::NEG_INFINITY
                } else {
                    f64::NAN
                }
            })
            .collect();

        let w_start = v_end;
        let w_end = v_end + s_info.gate_num_v as usize;
        let w: Vec<f64> = data[w_start..w_end]
            .iter()
            .map(|&x| {
                if x >= 2 {
                    if v_reso == 2 {
                        (x as f64 - 2.0) / 2.0 - 63.5
                    } else if v_reso == 4 {
                        (x as f64 - 2.0) - 127.0
                    } else {
                        f64::NAN
                    }
                } else {
                    0.0
                }
            })
            .collect();
        temp_data.push((azimuth, r, v, w));

        if s_info.radial_state == 2 || s_info.radial_state == 4 {
            elevations.push(elevation);
            let (mut tempaz, mut tempr, mut tempv, mut tempsw) = (
                Vec::with_capacity(temp_data.len()),
                Vec::with_capacity(temp_data.len()),
                Vec::with_capacity(temp_data.len()),
                Vec::with_capacity(temp_data.len()),
            );
            for (azimuth, r, v, sw) in temp_data {
                tempaz.push(azimuth);
                tempr.push(r);
                tempv.push(v);
                tempsw.push(sw);
            }

            azimuths.push(tempaz);
            REF.push(tempr);
            VEL.push(tempv);
            SW.push(tempsw);

            temp_data = Vec::new();
        }
    }
    let mut out_data = StandardData::default();
    out_data
        .attributes
        .insert("site_name".to_string(), site_name);
    out_data
        .attributes
        .insert("site_code".to_string(), site_code);
    out_data
        .attributes
        .insert("site_latitude".to_string(), centerlat.to_string());
    out_data
        .attributes
        .insert("site_longitude ".to_string(), centerlon.to_string());
    out_data
        .attributes
        .insert("site_altitude".to_string(), site_altitude.to_string());
    out_data
        .attributes
        .insert("site_type".to_string(), radartype);
    out_data
        .attributes
        .insert("task".to_string(), "VCP".to_string() + &vcp_mode);
    out_data
        .attributes
        .insert("r_reso".to_string(), Rreso.to_string());
    out_data
        .attributes
        .insert("v_reso".to_string(), Vreso.to_string());
    out_data.azimuth = azimuths;
    out_data.elevations = elevations;
    out_data.data.insert("REF".to_string(), REF.clone());
    out_data.data.insert("VEL".to_string(), VEL.clone());
    out_data.data.insert("SW".to_string(), SW.clone());
    info!("read completed");
    Ok(out_data)
}

#[allow(non_snake_case, unused_variables, unused_assignments)]
pub fn FMT_SAB_reader(path: &str) -> Result<StandardData, Box<dyn Error>> {
    let data = std::fs::read(path).unwrap_or_else(|e| {
        error!("failed to read file: {}", e);
        std::process::exit(1);
    });
    let mut cursor = Cursor::new(data);
    let commonblocks: CommonBlocks = cursor.read_le()?;
    let site_code = String::from_utf8_lossy(commonblocks.site_config.site_code.as_ref())
        .trim_end_matches('\0')
        .to_string();
    let radarinfo = io::base::get_radar_info();
    let station = &radarinfo[site_code.clone()];
    let site_name = station[0].as_str().unwrap().to_string();
    let centerlon = station[1].as_f64().unwrap();
    let centerlat = station[2].as_f64().unwrap();
    let radartype = io::base::get_type(commonblocks.site_config.radar_type).to_string();
    let site_altitude = station[4].as_f64().unwrap();
    let vcp_mode = String::from_utf8_lossy(commonblocks.task_config.task_name.as_ref())
        .trim_end_matches('\0')
        .to_string();
    println!("{:?}", vcp_mode);

    let mut fmt_sab_data = Vec::new();
    // todo error process
    while let Ok(d) = cursor.read_le::<RadialBlock>() {
        fmt_sab_data.push(d);
        if fmt_sab_data.last().unwrap().radial_header.radial_state == 4 {
            break;
        }
    }
    let mut out_data = StandardData::default();
    out_data
        .attributes
        .insert("site_name".to_string(), site_name);
    out_data
        .attributes
        .insert("site_code".to_string(), site_code);
    out_data
        .attributes
        .insert("site_latitude".to_string(), centerlat.to_string());
    out_data
        .attributes
        .insert("site_longitude ".to_string(), centerlon.to_string());
    out_data
        .attributes
        .insert("site_altitude".to_string(), site_altitude.to_string());
    out_data
        .attributes
        .insert("site_type".to_string(), radartype);
    out_data.attributes.insert("task".to_string(), vcp_mode);
    Ok(out_data)
}
