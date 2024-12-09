use binread::prelude::*;
use std::{collections::HashMap, default::Default, io::Cursor, vec};
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
pub struct CommonBlocks {
    pub generic_header: GenericHeader,
    pub site_config: Siteconfig,
    pub task_config: Taskconfig,
    #[br(count = task_config.cut_number)]
    pub cut_config: Vec<Cutconfig>,
}

#[derive(Debug, BinRead)]
pub struct GenericHeader {
    pub magic_num: i32,
    pub major_version: u16,
    pub minor_version: u16,
    pub generic_type: i32,
    pub product_type: i32,
    #[br(count = 16)]
    pub reserved: Vec<u8>,
}

#[derive(Debug, BinRead)]
pub struct Siteconfig {
    #[br(count = 8)]
    pub site_code: Vec<u8>,
    #[br(count = 32)]
    pub site_name: Vec<u8>,
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
    pub trans_loss: i16,
    pub recv_loss: i16,
    pub other_loss: i16,
    #[br(count = 46)]
    pub res2: Vec<u8>,
}

#[allow(non_snake_case)]
#[derive(Debug, BinRead)]
pub struct Taskconfig {
    #[br(count = 32)]
    pub task_name: Vec<u8>,
    #[br(count = 128)]
    pub task_dsc: Vec<u8>,
    pub polar_type: i32,
    pub scan_type: i32,
    pub pulse_width: i32,
    pub scan_start_time: i32,
    pub cut_number: i32,
    pub hori_noise: f32,
    pub vert_noise: f32,
    pub hori_cali: f32,
    pub vert_cali: f32,
    pub hori_tmp: f32,
    pub vert_tmp: f32,
    pub ZDR_cali: f32,
    pub PHIDP_cali: f32,
    pub LDR_cali: f32,
    #[br(count = 40)]
    pub res3: Vec<u8>,
}

#[derive(Debug, BinRead)]
pub struct Cutconfig {
    pub process_mode: i32,
    pub wave_form: i32,
    pub prf1: f32,
    pub prf2: f32,
    pub dealias_mode: i32,
    pub azimuth: f32,
    pub elev: f32,
    pub start_angle: f32,
    pub end_angle: f32,
    pub angular_reso: f32,
    pub scan_spd: f32,
    pub log_reso: i32,
    pub dop_reso: i32,
    pub max_range1: i32,
    pub max_range2: i32,
    pub start_range: i32,
    pub sample1: i32,
    pub sample2: i32,
    pub phase_mode: i32,
    pub atmos_loss: f32,
    pub nyquist_spd: f32,
    pub moments_mask: i64,
    pub moments_size_mask: i64,
    pub misc_filter_mask: i32,
    pub sqi_thres: f32,
    pub sig_thres: f32,
    pub csr_thres: f32,
    pub log_thres: f32,
    pub cpa_thres: f32,
    pub pmi_thres: f32,
    pub dplog_thres: f32,
    #[br(count = 4)]
    pub res_thres: Vec<u8>,
    pub dbt_mask: i32,
    pub dbz_mask: i32,
    pub vel_mask: i32,
    pub sw_mask: i32,
    pub dp_mask: i32,
    #[br(count = 12)]
    pub res_mask: Vec<u8>,
    pub scan_sync: i32,
    pub direction: i32,
    pub ground_clutter_classifier_type: i16,
    pub ground_clutter_filter_type: i16,
    pub ground_clutter_filter_notch_width: i16,
    pub ground_clutter_filter_window: i16,
    #[br(count = 72)]
    pub res4: Vec<u8>,
}

#[derive(Debug, BinRead)]
pub struct RadialBlock {
    pub radial_header: RadialHeader,
    #[br(count = radial_header.moment_number)]
    pub moment_data: Vec<MomentData>,
}

#[derive(Debug, BinRead)]
pub struct RadialHeader {
    pub radial_state: i32,
    pub spot_blank: i32,
    pub seq_number: i32,
    pub radial_number: i32,
    pub elevation_number: i32,
    pub azimuth: f32,
    pub elevation: f32,
    pub seconds: i32,
    pub microseconds: i32,
    pub data_length: i32,
    pub moment_number: i32,
    pub res5: i16,
    pub hori_est_noise: i16,
    pub vert_est_noise: i16,
    pub zip_type: u8,
    #[br(count = 13)]
    pub res6: Vec<u8>,
}
#[derive(Debug, BinRead)]
pub struct MomentData {
    pub data_type: i32,
    pub scale: i32,
    pub offset: i32,
    pub bin_length: i16,
    pub flags: i16,
    pub length: i32,
    #[br(count = 12)]
    pub res: Vec<u8>,
    #[br(count = length)]
    pub data: Vec<u8>,
}
