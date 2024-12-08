use binread::prelude::*;
use std::{collections::HashMap, default::Default, vec};
use tracing::error;
#[derive(Debug)]
pub struct StandardData {
    pub attributes: HashMap<String, String>,
    pub elevations: Vec<f64>,
    pub azimuth: Vec<Vec<f64>>,
    pub distance: Vec<Vec<Vec<Vec<f64>>>>,
    pub data: HashMap<String, Vec<Vec<Vec<f64>>>>,
}
#[allow(dead_code)]
impl StandardData {
    pub fn get_reso(&self, dtype: &str) -> Result<f64, Box<dyn std::error::Error>> {
        let reso = if dtype == "REF" {
            self.attributes["r_reso"].parse::<f64>()?
        } else {
            self.attributes["v_reso"].parse::<f64>()?
        };
        Ok(reso)
    }
    pub fn get_tilt(&self, tilt: usize) -> Result<f64, Box<dyn std::error::Error>> {
        if tilt < self.elevations.len() {
            Ok(self.elevations[tilt])
        } else {
            error!("tilt {} not in range", tilt);
            std::process::exit(1);
        }
    }
    pub fn get_tilt_all(&self) -> Result<Vec<f64>, Box<dyn std::error::Error>> {
        Ok(self.elevations.clone())
    }
    pub fn get_azimuth(&self, tilt: usize) -> Result<Vec<f64>, Box<dyn std::error::Error>> {
        Ok(self.azimuth[tilt].clone())
    }
    pub fn get_data(
        &self,
        tilt: usize,
        drange: f64,
        dtype: &str,
    ) -> Result<Vec<Vec<f64>>, Box<dyn std::error::Error>> {
        let reso = self.get_reso(dtype)?;
        if tilt >= self.data[dtype].len() {
            error!("tilt {} not in range", tilt);
            std::process::exit(1);
        }
        let data_tilt = &self.data[dtype][tilt];
        let dmax = data_tilt[0].len() as f64 * reso;
        if drange > dmax {
            error!("{}km out of dmax {}km", drange, dmax);
            std::process::exit(1);
        } else if drange == dmax {
            return Ok(data_tilt.to_vec());
        } else {
            let clip_range = (drange * reso) as usize;
            let out_data = data_tilt
                .iter()
                .map(|x| x[0..clip_range].to_vec())
                .collect();
            Ok(out_data)
        }
    }
}

impl Default for StandardData {
    fn default() -> Self {
        let mut attributes = HashMap::new();
        attributes.insert("scan_time".to_string(), "2020-05-17 11:00:28".to_string());
        attributes.insert("site_code".to_string(), "Z9532".to_string());
        attributes.insert("site_name".to_string(), "青岛".to_string());
        attributes.insert("site_longitude".to_string(), "120.23028".to_string());
        attributes.insert("site_latitude".to_string(), "35.98861".to_string());
        attributes.insert("site_altitude".to_string(), "35.1".to_string());
        attributes.insert("site_type".to_string(), "SA".to_string());
        attributes.insert("task".to_string(), "VCP21D".to_string());
        attributes.insert("r_reso".to_string(), "1.00".to_string());
        attributes.insert("v_reso".to_string(), "0.25".to_string());
        attributes.insert("nyquist_vel".to_string(), "8.37801".to_string());
        Self {
            attributes: attributes,
            elevations: vec![],
            azimuth: vec![],
            distance: vec![],
            data: HashMap::new(),
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

#[derive(Debug, BinRead)]
#[allow(non_camel_case_types)]
pub struct FMT_SAB {
    pub common_blocks: CommonBlocks,
    pub radial_blocks: u8, //res
}

#[derive(Debug, BinRead)]
pub struct CommonBlocks {
    pub generic_header: GenericHeader,
    pub site_config: Siteconfig,
    pub task_config: Taskconfig,
    pub cut_configs: Cutconfig,
}

#[derive(Debug, BinRead)]
pub struct GenericHeader {
    #[br(count = 32)]
    pub spare: Vec<u8>,
}

#[derive(Debug, BinRead)]
pub struct Siteconfig {
    pub site_code: [u8; 8],
    pub site_name: [u8; 32],
    pub latitude: f32,
    pub longitude: f32,
    pub antenna_height: i32,
    pub ground_height: i32,
    pub frequency: f32,
    pub beam_width_hori: f32,
    pub beam_width_vert: f32,
    pub rda_version: i32,
    pub radar_type: i16,
    pub antenna_gain: i16,
}

#[derive(Debug, BinRead)]
pub struct Taskconfig {
    #[br(count = 32)]
    pub task_name: Vec<u8>,
    #[br(count = 128)]
    pub task_description: Vec<u8>,
    pub polarization_type: i32,
    pub scan_type: i32,
    pub pulse_width: i32,
    pub scan_start_time: i32,
    pub cut_number: i32,
    pub horizontal_noise: f32,
    pub vertical_noise: f32,
    pub horizontal_calibration: f32,
    pub vertical_calibration: f32,
    pub horizontal_noise_temperature: f32,
    pub vertical_noise_temperature: f32,
    pub zdr_calibration: f32,
    pub phidp_calibration: f32,
    pub ldr_calibration: f32, 
    #[br(count = 40)]
    pub res: Vec<u8>,
}

#[derive(Debug, BinRead)]
pub struct Cutconfig {
    #[br(count = 256)]
    pub config: Vec<u8>,
}

#[derive(Debug, BinRead)]
pub struct RadialBlock {
    pub radial_header: RadialHeader,
    #[br(count = radial_header.length_of_data)]
    pub moment_data: Vec<u8>,
}

#[derive(Debug, BinRead)]
pub struct RadialHeader {
    pub radial_state: i32,
    pub spot_blank: i32,
    pub sequence_number: i32,
    pub radial_number: i32,
    pub elevation_number: i32,
    pub azimuth: f32,
    pub elevation: f32,
    pub seconds: i32,
    pub microseconds: i32,
    pub length_of_data: i32, //数据长度
    pub moment_number: i32, //类型数量 
    #[br(count = 2)]
    pub res1: Vec<u8>,
    pub horizontal_noise: i16,
    pub vertical_noise: i16,
    #[br(count = 14)]
    pub res2: Vec<u8>,
}
