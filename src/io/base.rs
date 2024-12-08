use serde_json::Value;

pub fn get_radar_info() -> Value {
    const STATION_JSON: &str = include_str!("../data/radar_station.json");
    let data: Value = serde_json::from_str(STATION_JSON).expect("站点数据读取失败");
    data
}

pub fn infer_type(filename: &str) -> Result<String, std::io::Error> {
    let mut code = String::new();
    // 从文件名读站号
    if filename.starts_with("RADA") {
        let spart: Vec<&str> = filename.split("-").collect();
        if spart.len() > 2 {
            code = spart[1].to_string();
        }
    } else if filename.starts_with("Z") {
        let spart: Vec<&str> = filename.split("_").collect();
        if spart.len() > 7 {
            code = spart[3].to_string();
        }
    }
    Ok(code)
}

pub fn get_type(type_id: i16) -> &'static str {
    match type_id {
        1 => "SA",
        2 => "SB",
        3 => "SC",
        4 => "SAD",
        5 => "SBD",
        6 => "SCD",
        33 => "CA",
        34 => "CB",
        35 => "CC",
        36 => "CCJ",
        37 => "CD",
        38 => "CAD",
        39 => "CBD",
        40 => "CCD",
        41 => "CCJD",
        42 => "CDD",
        65 => "XA",
        66 => "XAD",
        _ => "Unknown",
    }
}
