use crate::visualize::colormap;
use image::{ImageBuffer, Rgba};

pub fn ppi(grid: Vec<Vec<f64>>, grid_cols: usize, grid_rows: usize, fname: &str, dtype: &str) {
    let mut plt = ImageBuffer::new(grid_cols as u32, grid_rows as u32);
    let mut cmap_list: &str = include_str!("../data/colormap/REF.cmap");
    if dtype == "VEL" {
        cmap_list = include_str!("../data/colormap/VEL.cmap")
    }
    let color_palette = |value: f64| -> Option<Rgba<u8>> {
        if value == 0.0 || value.is_nan() {
            return Some(Rgba([0, 0, 0, 255]));
        }
        if value == f64::NEG_INFINITY && dtype == "VEL" {
            // RF
            return Some(Rgba([102, 0, 102, 255]));
        }
        let color_map = colormap::Cmap::from_list(&cmap_list).points;
        if value <= color_map.first().unwrap().0 {
            return Some(color_map.first().unwrap().1);
        }

        if value >= color_map.last().unwrap().0 {
            return Some(color_map.last().unwrap().1);
        }
        for i in 1..color_map.len() {
            let (low, low_color) = color_map[i - 1];
            let (high, high_color) = color_map[i];
            if value >= low && value < high {
                let factor = (value - low) / (high - low);
                let r = (low_color[0] as f64
                    + factor * (high_color[0] as f64 - low_color[0] as f64))
                    as u8;
                let g = (low_color[1] as f64
                    + factor * (high_color[1] as f64 - low_color[1] as f64))
                    as u8;
                let b = (low_color[2] as f64
                    + factor * (high_color[2] as f64 - low_color[2] as f64))
                    as u8;
                return Some(Rgba([r, g, b, 255]));
            }
        }
        None
    };

    for (y, row) in grid.iter().enumerate() {
        for (x, &value) in row.iter().enumerate() {
            if let Some(color) = color_palette(value) {
                plt.put_pixel(x as u32, y as u32, color);
            }
        }
    }

    plt.save(fname).unwrap();
}
