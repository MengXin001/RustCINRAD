use ndarray::Array2;
use std::io::{Cursor, Read, Seek, SeekFrom};

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

pub fn SAB_reader(data: &Vec<u8>) -> Result<(), bincode::Error> {
    let count: usize = data.len() / 2432; // SAB
    let mut reshaped = reshape(&data, count, 2432);
    let mut reshaped_vec: Vec<Vec<u8>> = Vec::new();
    for row in reshaped.axis_iter(ndarray::Axis(0)) {
        reshaped_vec.push(row.to_vec());
    }
    // 应该弄成function
    const con: f64 = (180.0 / 4096.0) * 0.125;
    let el_num: Vec<u16> = parse_data(&reshaped_vec, 44, count);
    let first_gate_r: Vec<u16> = parse_data(&reshaped_vec, 46, count);
    let first_gate_v: Vec<u16> = parse_data(&reshaped_vec, 48, count);
    let gate_length_r: Vec<u16> = parse_data(&reshaped_vec, 50, count);
    let gate_length_v: Vec<u16> = parse_data(&reshaped_vec, 52, count);
    let gate_num_r: Vec<u16> = parse_data(&reshaped_vec, 54, count);
    let gate_num_v: Vec<u16> = parse_data(&reshaped_vec, 56, count);
    // ... //
    // el az应该也弄成function f32精度不够
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
    println!("{:?}", azimuth);

    Ok(())
}
