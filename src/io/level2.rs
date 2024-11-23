use binread::prelude::*;
use std::error::Error;
use std::io::Cursor;

use crate::io;
use crate::io::dtype::*;
#[allow(non_snake_case)]
#[allow(unused_variables)]
pub fn SAB_reader(path: &str) -> Result<StandardData, Box<dyn Error>> {
    const CON: f64 = (180.0 / 4096.0) * 0.125;
    let data = std::fs::read(path).expect("文件读取失败");
    let radar_code = io::base::infer_type(path).unwrap();
    let radarinfo = io::base::get_radar_info();
    let station = &radarinfo[radar_code.clone()];
    let site_name = station[0].as_str().unwrap().to_string();
    let centerlon = station[1].as_f64().unwrap();
    let centerlat = station[2].as_f64().unwrap();
    let radartype = station[3].as_str().unwrap().to_string();
    let radarheight = station[4].as_f64().unwrap();

    let radial_num = 2432;
    let count: usize = data.len() / radial_num; // SAB
    let mut v_reso: u16 = 2;
    let mut vcp_mode: String = "Unknown".to_string();

    let mut temp_data: Vec<(f64, Vec<f64>, Vec<f64>, Vec<f64>)> = Vec::with_capacity(count);
    let mut r_distances = Vec::new();
    let mut v_distances = Vec::new();
    let mut sw_distances = Vec::new();
    let mut elevations = Vec::new();
    let mut REF = Vec::new();
    let mut VEL = Vec::new();
    let mut azimuths = Vec::new();
    let mut SW = Vec::new();

    for i in 0..count {
        let s = i * radial_num;
        let e = (i + 1) * radial_num;
        let mut cursor = Cursor::new(&data[s..e]);
        let sab_dtype: SAB_dtype = cursor.read_le()?;
        let s_info: S_INFO = sab_dtype.s_info;
        let sab_data: SAB_DATA = sab_dtype.sab_data;
        if i == 0 {
            v_reso = s_info.v_reso;
        }
        let Rreso = s_info.gate_length_r / 1000;
        let Vreso = s_info.gate_length_v / 1000;
        let elevation: f64 = s_info.elevation as f64 * CON;
        let azimuth: f64 = s_info.azimuth as f64 * CON;
        vcp_mode = s_info.vcp_mode.to_string();
        let r_start = s + 128;
        let r_end = r_start + s_info.gate_num_r as usize;

        let r: Vec<f64> = data[r_start..r_end]
            .iter()
            .filter_map(|&x| {
                let value = (x as f64 - 2.0) / 2.0 - 32.0;
                if value >= 0.0 {
                    Some(value)
                } else {
                    Some(0.0)
                }
            })
            .collect();

        let v_start = r_end;
        let v_end = v_start + s_info.gate_num_v as usize;
        let v: Vec<f64> = data[v_start..v_end]
            .iter()
            .filter_map(|&x| {
                let mut value = 0.0;
                if v_reso == 2 {
                    value = (x as f64 - 2.0) / 2.0 - 63.5;
                } else if v_reso == 4 {
                    value = (x as f64 - 2.0) - 127.0;
                }
                if value >= 0.0 {
                    Some(value)
                } else {
                    Some(0.0)
                }
            })
            .collect();

        let w_start = v_end;
        let w_end = v_end + s_info.gate_num_v as usize;
        let w: Vec<f64> = data[w_start..w_end]
            .iter()
            .filter_map(|&x| {
                let mut value = 0.0;
                if v_reso == 2 {
                    value = (x as f64 - 2.0) / 2.0 - 63.5;
                } else if v_reso == 4 {
                    value = (x as f64 - 2.0) - 127.0;
                }
                if value >= 0.0 {
                    Some(value)
                } else {
                    Some(0.0)
                }
            })
            .collect();
        temp_data.push((azimuth, r, v, w));

        let r_distance: Vec<f64> = (0..s_info.gate_num_r)
            .map(|x| x as f64 * s_info.gate_length_r as f64)
            .collect();

        let v_distance: Vec<f64> = (0..s_info.gate_num_v)
            .map(|x| x as f64 * s_info.gate_length_v as f64)
            .collect();

        let sw_distance: Vec<f64> = (0..s_info.gate_num_v)
            .map(|x| x as f64 * s_info.gate_length_v as f64)
            .collect();

        r_distances.push(r_distance);
        v_distances.push(v_distance);
        sw_distances.push(sw_distance);
        if s_info.radial_state == 2 || s_info.radial_state == 4 {
            elevations.push(elevation);
            let mut tempaz = Vec::with_capacity(temp_data.len());
            let mut tempr = Vec::with_capacity(temp_data.len());
            let mut tempv = Vec::with_capacity(temp_data.len());
            let mut tempsw = Vec::with_capacity(temp_data.len());
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
    out_data.site_name = site_name;
    out_data.site_latitude = centerlat;
    out_data.site_longitude = centerlon;
    out_data.site_altitude = radarheight;
    out_data.site_type = radartype;
    out_data.task = "VCP".to_string() + &vcp_mode;
    out_data.azimuth = azimuths;
    out_data.elevations = elevations;
    out_data.data = vec![REF.clone(), VEL.clone(), SW.clone()];
    print!("read completed");
    Ok(out_data)
}
