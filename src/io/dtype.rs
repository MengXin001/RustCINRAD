use binread::prelude::*;
use std::default::Default;

#[derive(Debug)]
pub struct StandardData {
    pub range: String,
    pub scan_time: String,
    pub site_code: String,
    pub site_name: String,
    pub site_type: String,
    pub site_longitude: f64,
    pub site_latitude: f64,
    pub site_altitude: f64,
    pub tangential_reso: String,
    pub nyquist_vel: String,
    pub task: String,
    pub elevations: Vec<f64>,
    pub azimuth: Vec<Vec<f64>>,
    pub distance: Vec<Vec<Vec<Vec<f64>>>>,
    pub data: Vec<Vec<Vec<Vec<f64>>>>,
}

impl Default for StandardData {
    fn default() -> Self {
        Self {
            range: "230".to_string(),
            scan_time: "2020-05-17 11:00:28".to_string(),
            site_code: "Z9532".to_string(),
            site_name: "青岛".to_string(),
            site_type: "SA".to_string(),
            site_longitude: 120.23028,
            site_latitude: 35.98861,
            site_altitude: 35.1,
            tangential_reso: "0.25".to_string(),
            nyquist_vel: "8.37801".to_string(),
            task: "VCP21D".to_string(),
            elevations: vec![],
            azimuth: vec![],
            distance: vec![],
            data: vec![],
        }
    }
}

#[derive(Debug, BinRead)]
#[allow(non_camel_case_types)]
pub struct SAB_dtype {
    pub s_header: S_HEADER,
    pub s_info: S_INFO,
    pub sab_data: SAB_DATA,
}
#[derive(Debug, BinRead)]
#[allow(non_camel_case_types)]
pub struct SAB_DATA {
    #[br(count = 460)]
    pub r: Vec<u8>,
    #[br(count = 920)]
    pub v: Vec<u8>,
    #[br(count = 920)]
    pub w: Vec<u8>,
    #[br(count = 4)]
    pub res4: Vec<u8>,
}
#[derive(Debug, BinRead)]
#[allow(non_camel_case_types)]
pub struct S_HEADER {
    #[br(count = 14)]
    pub spare: Vec<u8>,
    pub a: u16,
    #[br(count = 12)]
    pub res1: Vec<u8>,
}
#[derive(Debug, BinRead)]
#[allow(non_camel_case_types)]
pub struct S_INFO {
    pub time: u32,
    pub day: u16,
    pub unambiguous_distance: u16,
    pub azimuth: u16,
    pub radial_num: u16,
    pub radial_state: u16,
    pub elevation: u16,
    pub el_num: u16,
    pub first_gate_r: u16,
    pub first_gate_v: u16,
    pub gate_length_r: u16,
    pub gate_length_v: u16,
    pub gate_num_r: u16,
    pub gate_num_v: u16,
    pub sector_num: u16,
    pub system_coff: u32,
    pub r_pointer: u16,
    pub v_pointer: u16,
    pub w_pointer: u16,
    pub v_reso: u16,
    pub vcp_mode: u16,
    #[br(count = 8)]
    pub res2: Vec<u8>,
    pub r_pointer_2: u16, //？only god knows what
    pub v_pointer_2: u16, //？only god knows what
    pub w_pointer_2: u16, //？only god knows what
    pub nyquist_vel: u16,
    #[br(count = 38)]
    pub res3: Vec<u8>,
}
