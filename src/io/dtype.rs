use binread::prelude::*;
use std::{collections::HashMap, default::Default};
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