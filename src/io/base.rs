use std::fs::File;
use std::io::Read;
use serde_json::{Result, Value};

pub fn get_radar_info() -> Value {
    let mut file = File::open("./data/radar_station.json").expect("站点数据缺失");
    let mut buffer = String::new();
    let _ = file.read_to_string(&mut buffer);
    let data : Value = serde_json::from_str(&buffer).expect("站点数据读取失败"); 
    data
}
