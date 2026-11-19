# RustCINRAD

Decode CINRAD (China New Generation Weather Radar) data

This project is a covered work of [PyCINRAD](https://github.com/CyanideCN/PyCINRAD/), distributed under the terms of the GNU General Public License 3.0.


## Build

```powershell
cargo build --release

./RustCINRAD inspect Z_RADR_I_Z1234_11451419198100_O_DOR_CC_CAP.bin.bz2
./RustCINRAD plot Z_RADR_I_Z1234_11451419198100_O_DOR_CC_CAP.bin.bz2
./RustCINRAD plot Z_RADR_I_Z1234_11451419198100_O_DOR_CC_CAP.bin.bz2 --dtype VEL --tilt 1
./RustCINRAD plot Z_RADR_I_Z1234_11451419198100_O_DOR_CC_CAP.bin.bz2 --dtype CR
./RustCINRAD plot Z_RADR_I_Z1234_11451419198100_O_DOR_CC_CAP.bin.bz2 --dtype CAPPI --height-m 3000
./RustCINRAD plot Z_RADR_I_Z1234_11451419198100_O_DOR_CC_CAP.bin.bz2 --dtype RR --png rain-rate.png
./RustCINRAD volume Z_RADR_I_Z1234_11451419198100_O_DOR_CC_CAP.bin.bz2 --geotiff volume.tif
./RustCINRAD plot DUAL_POL.bin --dtype REF --attenuation --band C --png corrected.png
./RustCINRAD batch A.bin B.bin --dtype ET --output-dir output
./RustCINRAD accumulate EARLIER.bin LATER.bin --png rain.png --geotiff rain.tif
./RustCINRAD section Z_RADR_I_Z1234_11451419198100_O_DOR_CC_CAP.bin.bz2 --path 114 38.5 115 39
```

- `--dtype` 支持 `REF/TREF/VEL/SW/ZDR/RHO/PHI/KDP/HCL/CR/ET/VIL/VILD/CAPPI/RR/RAIN`，实际可用要素由基数据决定。`inspect` 可查看 `scan N: ... deg` 及每层可用要素。
- 不指定 `--tilt` 时，选择基数据中第一个包含该要素的层。

### Format

| 支持格式                             |
|-------------------------------------|
| SAB/SA/SB、CAB/CA/CB                |
| CC                                  |
| CD/SC                               |
| CC2                                 |
| RSTM/FMT/标准相控阵                  |
| PA相控阵                            |
| SWAN                                |
| MOC                                 |
| PUP/ROSE 径向产品                    |
| 旧 PUP/PUP 栅格/表格/风暴跟踪/冰雹/CAPPI/VWP产品 |

### Log
```powershell
$env:RUST_LOG = "rustcinrad=debug,rustcinrad_app=debug"
$env:RUST_LOG = "warn"

```
## Package
```rust,no_run
use rustcinrad::{read_path, ReadOptions, FieldId, projection::BeamModel};
let data = read_path(" Z_RADR_I_Z1234_11451419198100_O_DOR_CC_CAP.bin", &ReadOptions::default())?;
let beam = BeamModel::default();
let spec = data.grid_spec(0, FieldId::Reflectivity, beam)?;
let grid = data.grid(0, FieldId::Reflectivity, spec, beam)?;
rustcinrad::export::write_geotiff(&grid, "REF.tif")?;
# Ok::<(), rustcinrad::RadarError>(())
```
## Acknowledgements
The project is based on:
- [PyCINRAD](https://github.com/CyanideCN/PyCINRAD/)

The project is also inspired by:
- [metradar](https://github.com/nmcdev/metradar)
- [pycwr](https://github.com/YvZheng/pycwr)
