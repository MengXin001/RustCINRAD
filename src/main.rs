use std::fs::File;
use std::io::{Read, Seek, SeekFrom,Cursor};

fn infer_type(
    path: &str,
    filename: &str,
) -> Result<(Option<String>, Option<String>, String) , std::io::Error>{
    let mut file = File::open(path)?;
    let mut buffer = Vec::new();
    if file.read_to_end(&mut buffer).is_err() {
        println!("数据损坏 & 解析错误")
    }
    // 更多型号判断
    let data_len = buffer.len();
    let radial_num: i32;
    let radartype: String;
    let mut code: Option<String> = None;
    let mut detail_radartype: Option<String> = None;
    if data_len % 2432 == 0 {
        radial_num = 2432;
        radartype = "SAB".to_string();
    } else if data_len % 4132 == 0 {
        radial_num = 4132;
        radartype = "CB".to_string();
    } else {
        radial_num = 3132;
        radartype = "SC".to_string();
    }
    // 从文件名读站号
    if filename.starts_with("RADA") {
        let spart: Vec<&str> = filename.split("-").collect();
        if spart.len() > 2 {
            code = Some(spart[1].to_string());
            detail_radartype = Some(spart[2].to_string());
        }
    } else if filename.starts_with("Z") {
        let spart: Vec<&str> = filename.split("_").collect();
        if spart.len() > 7 {
            code = Some(spart[3].to_string());
            detail_radartype = Some(spart[7].to_string());
        }
    }
    Ok((code, detail_radartype, radartype))
}

fn SAB_reader(f: &str) -> Result<String, Box<dyn std::error::Error>> {
    const header_size: usize = 128;
    let mut file = File::open(f)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    //
    Ok((String::from("Success SABReader")))
}

fn main() {
    let filePath: &str = "Z_RADR_I_Z9515_20160623043100_O_DOR_SA_CAP.bin";
    let fileName: &str = "Z_RADR_I_Z9515_20160623043100_O_DOR_SA_CAP.bin";
    match infer_type(filePath, fileName) {
        Ok((code, dradartype, radartype)) => {
            println!("数据类型: {:?} / {:?}", radartype, dradartype.unwrap());
            println!("站号: {:?}", code.unwrap());
        }
        Err(error) => {
            println!("{:?}", error)
        }
    };
    match SAB_reader(filePath) {
        Ok(result) => {
            println!("{:?}", result);
        },
        Err(e) => {
            eprintln!("Error: {}", e);
        },
    }
}
