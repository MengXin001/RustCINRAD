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