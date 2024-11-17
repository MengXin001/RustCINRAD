use binread::prelude::*;
use std::collections::HashMap;
use std::error::Error;
use std::io::Cursor;

#[derive(Debug, BinRead)]
pub struct S_INFO {
    #[br(count = 14)]
    res0: Vec<u8>,
    flag: u16,
    #[br(count = 12)]
    res1: Vec<u8>,
    time: u32,
    day: u16,
    unambiguous_distance: u16,
    azimuth: u16,
    radial_num: u16,
    radial_state: u16,
    elevation: u16,
    el_num: u16,
    first_gate_r: u16,
    first_gate_v: u16,
    gate_length_r: u16,
    gate_length_v: u16,
    gate_num_r: u16,
    gate_num_v: u16,
    sector_num: u16,
    system_coff: u32,
    r_pointer: u16,
    v_pointer: u16,
    w_pointer: u16,
    v_reso: u16,
    vcp_mode: u16,
    r_pointer_2: u16, //？only god knows what
    v_pointer_2: u16, //？only god knows what
    w_pointer_2: u16, //？only god knows what
    nyquist_vel: u16,
    #[br(count = 38)]
    res3: Vec<u8>,
}

pub fn SAB_reader(path: &str) -> Result<(&str), Box<dyn Error>> {
    const CON: f64 = (180.0 / 4096.0) * 0.125;
    let data = std::fs::read(path).expect("文件读取失败");
    let count: usize = data.len() / 2432; // SAB
                                          // Rebuild todo start
    let radial_num = 2432;

    for i in 0..count {
        let s = i * radial_num;
        let e = (i + 1) * radial_num;
        let mut cursor = Cursor::new(&data[s..e]);
        let s_info: S_INFO = cursor.read_le()?;
        let Rreso = s_info.gate_length_r / 1000;
        let Vreso = s_info.gate_length_v / 1000;
        let elevation: f64 = s_info.elevation as f64 * CON;
        let azimuth: f64 = s_info.azimuth as f64 * CON;

        let r_start = s;
        let r_end = s + s_info.gate_length_r as usize;

        let r: Vec<f64> = data[r_start..r_end]
            .iter()
            .map(|&x| (x as f64 - 2.0) / 2.0 - 32.0)
            .filter(|&x| x >= 0.0)
            .collect();

        let v_start = r_end;
        let v_end = v_start + s_info.gate_length_v as usize;
        let v: Vec<f64> = data[v_start..v_end]
            .iter()
            .map(|&x| (x as f64 - 2.0) / 2.0 - 63.5)
            .filter(|&x| x >= 0.0)
            .collect();
    }
    Ok("read completed")
}
