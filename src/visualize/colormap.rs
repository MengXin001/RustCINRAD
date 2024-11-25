use image::Rgba;
pub struct Cmap {
    pub points: Vec<(f64, Rgba<u8>)>,
}

impl Cmap {
    pub fn from_list(list: &str) -> Cmap {
        let mut points = Vec::new();
        for line in list.lines().enumerate() {
            let trimmed = line.1.trim();
            if trimmed.is_empty() || trimmed.starts_with('*') {
                continue;
            }
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            let position: f64 = parts[0].parse().unwrap();
            let rgb: Vec<u8> = parts[1].split('/').map(|s| s.parse().unwrap()).collect();
            points.push((position, Rgba([rgb[0], rgb[1], rgb[2], 255])));
        }
        Cmap { points }
    }
}
