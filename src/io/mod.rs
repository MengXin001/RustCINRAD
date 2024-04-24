use bincode::{DefaultOptions, Options};
use serde::de::DeserializeOwned;
use std::io::{Cursor, Read, Seek, SeekFrom};
mod model;
pub use model::{SAB_dtype, S_DATA, S_HEADER, S_INFO, S_RES};

pub fn SAB_reader(data: &Vec<u8>) -> Result<SAB_dtype,bincode::Error> {
    let mut reader = Cursor::new(data);
    let data_header: S_HEADER = deserialize(&mut reader)?;
    let data_info: S_INFO = deserialize(&mut reader)?;
    let data_res: S_RES = deserialize(&mut reader)?;
    let data_data: S_DATA = deserialize(&mut reader)?;
    let mut file = SAB_dtype::new(data_header,data_info,data_res,data_data);
    Ok(file)
}

fn deserialize<R: Read + Seek, S: DeserializeOwned>(reader: &mut R) -> Result<S, bincode::Error> {
    Ok(DefaultOptions::new()
        .with_fixint_encoding()
        .with_big_endian()
        .deserialize_from(reader.by_ref())?)
}