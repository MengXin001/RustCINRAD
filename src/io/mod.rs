use core::slice;
use ndarray::{Array2, Array3};
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::mem;
fn reshape(data: &Vec<u8>, rows: usize, cols: usize) -> Array2<u8> {
    // numpy.reshape
    assert_eq!(data.len(), rows * cols);
    Array2::from_shape_vec((rows, cols), data.clone()).unwrap()
}

fn parse_data(reshaped_data: &Vec<Vec<u8>>, index: usize, count: usize) -> Vec<u16> {
    let mut data = Vec::with_capacity(count);
    for i in 0..count {
        let appened = (reshaped_data[i][index] as u16) + (reshaped_data[i][index + 1] as u16);
        data.push(appened);
    }
    data
}
#[derive(Debug)]
struct TempDtype {
    header: [u8; 128],
    r: Vec<u8>,
    v: Vec<u8>,
    s: Vec<u8>,
    res: Vec<u8>,
}

impl TempDtype {
    fn new(rnum: usize, vnum: usize, size: usize) -> Self {
        TempDtype {
            header: [0; 128],
            r: vec![0; rnum],
            v: vec![0; vnum],
            s: vec![0; vnum],
            res: vec![0; size - 128 - rnum - vnum * 2],
        }
    }
}

pub fn SAB_reader(path: &str) -> Result<(), bincode::Error> {
    const con: f64 = (180.0 / 4096.0) * 0.125;
    let mut data = std::fs::read(path).expect("文件读取失败");
    let count: usize = data.len() / 2432; // SAB
    let mut reshaped = reshape(&data, count, 2432);
    let mut reshaped_vec: Vec<Vec<u8>> = Vec::new();
    for row in reshaped.axis_iter(ndarray::Axis(0)) {
        reshaped_vec.push(row.to_vec());
    }

    let radial_num: Vec<u16> = parse_data(&reshaped_vec, 38, count); // 溢出了......
    let el_num: Vec<u16> = parse_data(&reshaped_vec, 44, count);
    let first_gate_r: Vec<u16> = parse_data(&reshaped_vec, 46, count);
    let first_gate_v: Vec<u16> = parse_data(&reshaped_vec, 48, count);
    let gate_length_r: Vec<u16> = parse_data(&reshaped_vec, 50, count);
    let gate_length_v: Vec<u16> = parse_data(&reshaped_vec, 52, count);
    let gate_num_r: Vec<u16> = parse_data(&reshaped_vec, 54, count);
    let gate_num_v: Vec<u16> = parse_data(&reshaped_vec, 56, count);

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

    let el = elevation[boundary[0]];
    let mut b = boundary.clone();
    b.push(count);
    let mut gnr = Vec::new();
    let mut gnv = Vec::new();
    for &i in &boundary {
        gnr.push(gate_num_r[i]);
        gnv.push(gate_num_v[i]);
    }

    let mut out_data: Vec<(Array3<u8>, Vec<f32>)> = Vec::new();
    let diff_b = b.windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>();
    let mut idx = 0;
    for (bidx, (rnum, vnum)) in diff_b.iter().zip(gnr.iter().zip(gnv.iter())) {
        let buffer: Vec<Vec<u8>> = reshaped_vec.clone();
        let temp_dtype = TempDtype::new(*rnum as usize, *vnum as usize, 2432);
        idx += 1;
        println!(
            "bidx: {}, rnum: {}, vnum: {}, idx: {}",
            bidx, rnum, vnum, idx
        ); //debug
    }

    Ok(())
}
