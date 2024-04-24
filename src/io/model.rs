use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;
pub struct SAB_dtype {
    header: S_HEADER,
    info: S_INFO,
    res: S_RES,
    data: S_DATA,
}

impl SAB_dtype {
    pub(crate) fn new(
        data_header: S_HEADER,
        data_info: S_INFO,
        data_res: S_RES,
        data_data: S_DATA,
    ) -> Self {
        Self {
            header: data_header,
            info: data_info,
            res: data_res,
            data: data_data,
        }
    }
    pub fn header(&self) -> &S_HEADER {
        &self.header
    }
    pub fn info(&self) -> &S_INFO {
        &self.info
    }
    pub fn res(&self) -> &S_RES {
        &self.res
    }
    pub fn data(&self) -> &S_DATA {
        &self.data
    }
}
#[repr(C)]
#[derive(Serialize, Deserialize, Debug)]
pub struct S_HEADER {
    // S_HEADER
    spare: [u16; 7],
    a: u16,
    res: [u16; 6],
}

impl S_HEADER {
    pub fn spare(&self) -> [u16; 7] {
        self.spare
    }

    pub fn a(&self) -> u16 {
        self.a
    }

    pub fn res(&self) -> [u16; 6] {
        self.res
    }
}
#[repr(C)]
#[derive(Serialize, Deserialize, Debug)]
pub struct S_INFO {
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
    res2: [u16; 4],
    r_pointer_2: u16,
    v_pointer_2: u16,
    w_pointer_2: u16,
    nyquist_vel: u16,
}

impl S_INFO {
    // S_INFO 字段太多，这里写的方法太抽象
    pub fn time(&self) -> u32 {
        self.time
    }

    pub fn day(&self) -> u16 {
        self.day
    }

    pub fn unambiguous_distance(&self) -> u16 {
        self.unambiguous_distance
    }

    pub fn azimuth(&self) -> u16 {
        self.azimuth
    }

    pub fn radial_num(&self) -> u16 {
        self.radial_num
    }

    pub fn radial_state(&self) -> u16 {
        self.radial_state
    }

    pub fn elevation(&self) -> u16 {
        self.elevation
    }

    pub fn el_num(&self) -> u16 {
        self.el_num
    }

    pub fn first_gate_r(&self) -> u16 {
        self.first_gate_r
    }

    pub fn first_gate_v(&self) -> u16 {
        self.first_gate_v
    }

    pub fn gate_length_r(&self) -> u16 {
        self.gate_length_r
    }

    pub fn gate_length_v(&self) -> u16 {
        self.gate_length_v
    }

    pub fn gate_num_r(&self) -> u16 {
        self.gate_num_r
    }

    pub fn gate_num_v(&self) -> u16 {
        self.gate_num_v
    }

    pub fn sector_num(&self) -> u16 {
        self.sector_num
    }

    pub fn system_coff(&self) -> u32 {
        self.system_coff
    }

    pub fn r_pointer(&self) -> u16 {
        self.r_pointer
    }

    pub fn v_pointer(&self) -> u16 {
        self.v_pointer
    }

    pub fn w_pointer(&self) -> u16 {
        self.w_pointer
    }

    pub fn v_reso(&self) -> u16 {
        self.v_reso
    }

    pub fn vcp_mode(&self) -> u16 {
        self.vcp_mode
    }

    pub fn res2(&self) -> &[u16; 4] {
        &self.res2
    }

    pub fn r_pointer_2(&self) -> u16 {
        self.r_pointer_2
    }

    pub fn v_pointer_2(&self) -> u16 {
        self.v_pointer_2
    }

    pub fn w_pointer_2(&self) -> u16 {
        self.w_pointer_2
    }

    pub fn nyquist_vel(&self) -> u16 {
        self.nyquist_vel
    }
}
#[repr(C)]
#[derive(Serialize, Deserialize, Debug)]
pub struct S_RES {
    res3: [u16; 19],
}

impl S_RES {
    pub fn res3(&self) -> [u16; 19] {
        self.res3
    }
}
#[repr(C)]
#[derive(Serialize, Deserialize, Debug)]
pub struct S_DATA {
    #[serde(with = "BigArray")]
    r: [u8; 460],
    #[serde(with = "BigArray")]
    v: [u8; 920],
    #[serde(with = "BigArray")]
    w: [u8; 920],
    res4: [u16; 2],
}

impl S_DATA {
    pub fn r(&self) -> [u8; 460] {
        self.r
    }
    pub fn v(&self) -> [u8; 920] {
        self.v
    }
    pub fn w(&self) -> [u8; 920] {
        self.w
    }
    pub fn res4(&self) -> [u16; 2] {
        self.res4
    }
}
