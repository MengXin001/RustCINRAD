use byteorder::{LittleEndian, ReadBytesExt};
use memmap::Mmap;
use std::fs::File;
use std::io::{Cursor, Read};

#[derive(Debug)]
struct S_HEADER {
    spare: [u16; 7],
    a: u16,
    res: [u16; 6],
}
#[derive(Debug)]
struct S_INFO {
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
#[derive(Debug)]
struct S_RES {
    res3: [u16; 19],
}
#[derive(Debug)]
struct SAB_DATA {
    r: Vec<u8>,
    v: Vec<u8>,
    w: Vec<u8>,
    res4: [u16; 2],
}
#[derive(Debug)]
struct SAB_Dtype {
    header: S_HEADER,
    info: S_INFO,
    res: S_RES,
    sab_data: SAB_DATA,
}

fn read_header<R: Read>(reader: &mut R) -> S_HEADER {
    let mut spare = [0; 7];
    for i in 0..7 {
        spare[i] = reader.read_u16::<LittleEndian>().unwrap();
    }
    let a = reader.read_u16::<LittleEndian>().unwrap();
    let mut res = [0; 6];
    for i in 0..6 {
        res[i] = reader.read_u16::<LittleEndian>().unwrap();
    }
    S_HEADER { spare, a, res }
}
fn read_info<R: Read>(reader: &mut R) -> S_INFO {
    S_INFO {
        time: reader.read_u32::<LittleEndian>().unwrap(),
        day: reader.read_u16::<LittleEndian>().unwrap(),
        unambiguous_distance: reader.read_u16::<LittleEndian>().unwrap(),
        azimuth: reader.read_u16::<LittleEndian>().unwrap(),
        radial_num: reader.read_u16::<LittleEndian>().unwrap(),
        radial_state: reader.read_u16::<LittleEndian>().unwrap(),
        elevation: reader.read_u16::<LittleEndian>().unwrap(),
        el_num: reader.read_u16::<LittleEndian>().unwrap(),
        first_gate_r: reader.read_u16::<LittleEndian>().unwrap(),
        first_gate_v: reader.read_u16::<LittleEndian>().unwrap(),
        gate_length_r: reader.read_u16::<LittleEndian>().unwrap(),
        gate_length_v: reader.read_u16::<LittleEndian>().unwrap(),
        gate_num_r: reader.read_u16::<LittleEndian>().unwrap(),
        gate_num_v: reader.read_u16::<LittleEndian>().unwrap(),
        sector_num: reader.read_u16::<LittleEndian>().unwrap(),
        system_coff: reader.read_u32::<LittleEndian>().unwrap(),
        r_pointer: reader.read_u16::<LittleEndian>().unwrap(),
        v_pointer: reader.read_u16::<LittleEndian>().unwrap(),
        w_pointer: reader.read_u16::<LittleEndian>().unwrap(),
        v_reso: reader.read_u16::<LittleEndian>().unwrap(),
        vcp_mode: reader.read_u16::<LittleEndian>().unwrap(),
        res2: {
            let mut res2 = [0; 4];
            for i in 0..4 {
                res2[i] = reader.read_u16::<LittleEndian>().unwrap();
            }
            res2
        },
        r_pointer_2: reader.read_u16::<LittleEndian>().unwrap(),
        v_pointer_2: reader.read_u16::<LittleEndian>().unwrap(),
        w_pointer_2: reader.read_u16::<LittleEndian>().unwrap(),
        nyquist_vel: reader.read_u16::<LittleEndian>().unwrap(),
    }
}
fn read_res<R: Read>(reader: &mut R) -> S_RES {
    let mut res3 = [0; 19];
    for i in 0..19 {
        res3[i] = reader.read_u16::<LittleEndian>().unwrap();
    }
    S_RES { res3 }
}
fn read_sab_data<R: Read>(reader: &mut R) -> SAB_DATA {
    let mut r = vec![0; 460];
    reader.read_exact(&mut r).unwrap();
    let mut v = vec![0; 920];
    reader.read_exact(&mut v).unwrap();
    let mut w = vec![0; 920];
    reader.read_exact(&mut w).unwrap();
    let mut res4 = [0; 2];
    for i in 0..2 {
        res4[i] = reader.read_u16::<LittleEndian>().unwrap();
    }
    SAB_DATA { r, v, w, res4 }
}

fn main() {
    let file = File::open("Z_RADR_I_Z9515_20160623043100_O_DOR_SA_CAP.bin").expect("expect");
    let file_size = file.metadata().unwrap().len() as usize;
    let mmap = unsafe { Mmap::map(&file).expect("expect") };

    let mut cursor = Cursor::new(&mmap[..file_size]);
    let header = read_header(&mut cursor);
    let info = read_info(&mut cursor);
    let res = read_res(&mut cursor);
    let sab_data = read_sab_data(&mut cursor);
    let sab_dtype = SAB_Dtype {
        header,
        info,
        res,
        sab_data,
    };

    let deltday = sab_dtype.info.day;
    let deltsec = sab_dtype.info.time;
    println!("Day: {:?}", deltday);
    println!("Time: {:?}", deltsec);
}