use ndarray::Array2;
use std::collections::HashMap;
use std::io::{Cursor, Read, Seek, SeekFrom};

fn reshape(data: &Vec<u8>, rows: usize, cols: usize) -> Array2<u8> {
    // numpy.reshape
    assert_eq!(data.len(), rows * cols);
    Array2::from_shape_vec((rows, cols), data.clone()).unwrap()
}

fn parse_data(reshaped_data: &Vec<Vec<u8>>, index: usize, count: usize) -> Vec<u16> {
    let mut data = Vec::with_capacity(count);
    for i in 0..count {
        let appened = (reshaped_data[i][index] as u16) + (reshaped_data[i][index + 1] as u16) * 256;
        data.push(appened);
    }
    data
}

pub fn SAB_reader(path: &str) -> (Vec<HashMap<String, Vec<f64>>>) {
    const con: f64 = (180.0 / 4096.0) * 0.125;
    let mut data = std::fs::read(path).expect("文件读取失败");
    let count: usize = data.len() / 2432; // SAB
    let mut reshaped = reshape(&data, count, 2432);
    let mut reshaped_vec: Vec<Vec<u8>> = Vec::new();
    for row in reshaped.axis_iter(ndarray::Axis(0)) {
        reshaped_vec.push(row.to_vec());
    }
    // Rebuild todo start
    let radial_num: Vec<u16> = parse_data(&reshaped_vec, 38, count);
    let gate_length_r: Vec<u16> = parse_data(&reshaped_vec, 50, count);
    let gate_length_v: Vec<u16> = parse_data(&reshaped_vec, 52, count);
    let Rreso = gate_length_r[0] / 1000;
    let Vreso = gate_length_v[0] / 1000;
    let gate_num_r: Vec<u16> = parse_data(&reshaped_vec, 54, count);
    let gate_num_v: Vec<u16> = parse_data(&reshaped_vec, 56, count);
    let v_reso: Vec<u16> = parse_data(&reshaped_vec, 70, count);
    let vcp_mode: Vec<u16> = parse_data(&reshaped_vec, 72, count);
    let elevation: Vec<f64> = (0..count)
        .map(|i| {
            let el = (reshaped_vec[i][42] as u16 + (reshaped_vec[i][43] as u16) * 256) as f64 * con;
            el
        })
        .collect();

    let azimuth: Vec<f64> = (0..count)
        .map(|i| {
            let az = (reshaped_vec[i][36] as u16 + (reshaped_vec[i][37] as u16) * 256) as f64 * con;
            az
        })
        .collect();

    let boundary: Vec<usize> = radial_num
        .iter()
        .enumerate()
        .filter(|&(_, &val)| val == 1)
        .map(|(index, _)| index)
        .collect();
    // Rebuild todo end
    let el = elevation[boundary[0]] * con;
    let dv = v_reso[0];
    let _header_size = 128;
    let mut b = boundary.clone();
    b.push(count);
    let mut gnr = Vec::new();
    let mut gnv = Vec::new();
    for &i in &boundary {
        gnr.push(gate_num_r[i]);
        gnv.push(gate_num_v[i]);
    }
    let diff_b = b.windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>();
    let mut idx = 0;
    let mut out_data: Vec<HashMap<String, Vec<f64>>> = Vec::new();
    for (bidx, (rnum, vnum)) in diff_b.iter().zip(gnr.iter().zip(gnv.iter())) {
        let mut f = &data[0..bidx * 2432];
        let header = &f[0.._header_size];
        let r: Vec<f64>;
        let v: Vec<f64>;
        let s: Vec<f64>;
        if *rnum != 0 {
            r = f[_header_size + 1..*rnum as usize + _header_size + 1]
                .to_vec()
                .iter()
                .map(|&x| (x as f64 - 2.0) / 2.0 - 32.0)
                .filter(|&x| x >= 0.0)
                .collect();
        } else {
            r = vec![0; *rnum as usize].iter().map(|&x| x as f64).collect();
        };
        if dv == 2 && *vnum != 0 {
            v = f[_header_size + 1..*vnum as usize + _header_size + 1]
                .to_vec()
                .iter()
                .map(|&x| (x as f64 - 2.0) / 2.0 - 63.5)
                .filter(|&x| x >= 0.0)
                .collect();
        } else {
            v = vec![0; *vnum as usize].iter().map(|&x| x as f64).collect();
        };
        //let v =
        out_data.resize(idx + 1, HashMap::new());
        out_data[idx].insert("REF".to_string(), r);
        out_data[idx].insert("VEL".to_string(), v);
        out_data[idx].insert(
            "azimuth".to_string(),
            azimuth[b[idx] as usize..b[idx + 1] as usize].to_vec(),
        );
        idx += 1;
        //println!(
        //  "bidx: {}, rnum: {}, vnum: {}, idx: {}",
        //   bidx, rnum, vnum, idx
        //);
    }
    println!("Read End");
    out_data
}
fn parse_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes.try_into().unwrap())
}

fn parse_u16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes(bytes.try_into().unwrap())
}
