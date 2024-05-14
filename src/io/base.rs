use std::fs::File;
use std::io::Read;
use serde_json::{Result, Value};

pub fn get_radar_info() -> Value {
    const STATION_JSON: &str = include_str!("../data/radar_station.json");
    let data : Value = serde_json::from_str(STATION_JSON).expect("站点数据读取失败"); 
    data
}
