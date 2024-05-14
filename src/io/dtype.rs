use std::fs::File;
use std::io::Read;

pub fn infer_type(filename: &str) -> Result<String, std::io::Error> {
    let mut code = String::new();
    let mut detail_radartype = String::new();
    // 从文件名读站号
    if filename.starts_with("RADA") {
        let spart: Vec<&str> = filename.split("-").collect();
        if spart.len() > 2 {
            code = spart[1].to_string();
            detail_radartype = spart[2].to_string();
        }
    } else if filename.starts_with("Z") {
        let spart: Vec<&str> = filename.split("_").collect();
        if spart.len() > 7 {
            code = spart[3].to_string();
            detail_radartype = spart[7].to_string();
        }
    }
    Ok(code)
}
