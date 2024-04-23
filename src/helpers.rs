// test use
use std::fs::File;
use std::path::Path;
use std::io::{Read, Seek, SeekFrom};

fn headReader<P: AsRef<Path>>(
    path: P,
    startOffset: u64,
    byteGain: usize,
) -> Result<Vec<u8>, std::io::Error> {
    let mut file = File::open(path)?;
    file.seek(std::io::SeekFrom::Start(startOffset))?;
    let mut headerBytes = vec![0u8; byteGain];
    file.read_exact(&mut headerBytes)?;
    Ok(headerBytes)
}
  
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let filePath = "Z_RADR_I_Z9xxx_202y0mdd160000_O_DOR_SAD_CAP_FMT.bin"; // Start with RSTM? Wtf of the radar type? 
    let startOffset = 14;
    let byteGain = 2;
    let headerBytes = headReader(filePath, startOffset, byteGain)?;
    for byte in headerBytes {
        print!("{:02X}", byte);
    }
    Ok(())
}