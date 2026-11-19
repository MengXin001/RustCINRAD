use std::{env, error::Error, fs, path::PathBuf};

type Color = [f64; 3];
fn color(text: &str) -> Result<Color, Box<dyn Error>> {
    let channels: Vec<_> = text.split('/').map(str::parse::<f64>).collect::<Result<_, _>>()?;
    if channels.len() != 3 || channels.iter().any(|x| !(0.0..=255.0).contains(x)) {
        return Err(format!("invalid colour {text}").into());
    }
    Ok([channels[0], channels[1], channels[2]])
}

fn bytes(c: Color) -> [u8; 3] {
    c.map(|x| x.clamp(0.0, 255.0) as u8)
}

fn lookup(knots: &[(f64, Color, Color)], listed: bool, count: usize) -> Vec<[u8; 3]> {
    if listed {
        return knots.iter().map(|p| bytes(p.1)).collect();
    }
    let first = knots[0].0;
    let last = knots[knots.len() - 1].0;
    (0..count)
        .map(|i| {
            if i == 0 {
                return bytes(knots[0].2);
            }
            if i == count - 1 {
                return bytes(knots[knots.len() - 1].1);
            }
            let x = first + (last - first) * i as f64 / (count - 1) as f64;
            let j = knots.partition_point(|p| p.0 < x).min(knots.len() - 1).max(1);
            let (a, b) = (knots[j - 1], knots[j]);
            let t = (x - a.0) / (b.0 - a.0);
            bytes(std::array::from_fn(|k| a.2[k] + (b.1[k] - a.2[k]) * t))
        })
        .collect()
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut generated = String::new();
    for name in ["REF", "VEL", "ET", "VIL", "ZDR", "KDP", "CC", "HCL", "SW", "OHP", "VEL_REVERSE", "SW_REVERSE"] {
        let path = format!("data/colormap/{name}.cmap");
        println!("cargo:rerun-if-changed={path}");
        let definition = fs::read_to_string(path)?;
        let mut listed = false;
        let mut under = [0.0; 3];
        let mut over = [0.0; 3];
        let mut knots: Vec<(f64, Color, Color)> = Vec::new();
        for line in definition.lines().filter(|l| !l.trim().is_empty()) {
            if let Some(header) = line.strip_prefix('*') {
                let (key, value) = header.split_once(':').ok_or("invalid cmap header")?;
                match key {
                    "TYPE" => listed = value == "LISTED",
                    "UNDER" => under = color(value)?,
                    "OVER" => over = color(value)?,
                    _ => {}
                }
                continue;
            }
            let parts: Vec<_> = line.split_whitespace().collect();
            let x: f64 = parts.first().ok_or("empty colour knot")?.parse()?;
            if listed {
                let c = color(parts.get(1).ok_or("missing colour")?)?;
                knots.push((x, c, c));
            } else {
                let left = match *parts.get(1).ok_or("missing left colour")? {
                    "BEGIN" => [0.0; 3],
                    "&" => knots.last().ok_or("no previous colour")?.2,
                    c => color(c)?,
                };
                let right = match *parts.get(2).ok_or("missing right colour")? {
                    "END" | "&" => left,
                    c => color(c)?,
                };
                knots.push((x, left, right));
            }
        }
        if knots.len() < 2 || knots.windows(2).any(|p| p[1].0 <= p[0].0) {
            return Err(format!("invalid colour knots: {name}").into());
        }
        let lut = lookup(&knots, listed, if listed { knots.len() } else { 256 });
        let bar = if listed {
            if name == "HCL" { lut.clone() } else { lut.iter().rev().copied().collect() }
        } else {
            let uniform: Vec<_> = knots.iter().rev().enumerate().map(|(i, p)| (i as f64, p.2, p.1)).collect();
            lookup(&uniform, false, 256)
        };
        if ["ET", "VIL", "ZDR", "KDP", "CC", "HCL", "OHP"].contains(&name) {
            generated.push_str(&format!("pub const {name}_CBAR: &[[u8;3]] = &{bar:?};\n"));
        }
        generated.push_str(&format!("pub const {name}: &[[u8;3]] = &{lut:?};\n"));
        if !name.ends_with("REVERSE") {
            generated.push_str(&format!(
                "pub const {name}_UNDER: [u8;3] = {:?};\npub const {name}_OVER: [u8;3] = {:?};\n",
                bytes(under),
                bytes(over)
            ));
        }
    }
    fs::write(PathBuf::from(env::var_os("OUT_DIR").ok_or("missing OUT_DIR")?).join("colormaps.rs"), generated)?;
    Ok(())
}
