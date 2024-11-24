use image::{ImageBuffer, Rgba};
pub fn ppi(grid: Vec<Vec<f64>>, grid_cols: usize, grid_rows: usize, fname: &str) {
    let mut plt = ImageBuffer::new(grid_cols as u32, grid_rows as u32);
    // todo rebuild
    let color_palette = |value: f64| -> Option<Rgba<u8>> {
        if value == 0.0 {
            return Some(Rgba([0, 0, 0, 255]));
        }
        let color_scale = vec![
            (0.0, Rgba([0, 0, 246, 255])),
            (5.0, Rgba([1, 160, 246, 255])),
            (10.0, Rgba([0, 236, 236, 255])),
            (15.0, Rgba([1, 255, 0, 255])),
            (20.0, Rgba([0, 200, 0, 255])),
            (25.0, Rgba([1, 144, 0, 255])),
            (30.0, Rgba([255, 255, 0, 255])),
            (35.0, Rgba([231, 192, 0, 255])),
            (40.0, Rgba([255, 144, 0, 255])),
            (45.0, Rgba([255, 0, 0, 255])),
            (50.0, Rgba([214, 0, 0, 255])),
            (55.0, Rgba([192, 0, 0, 255])),
            (60.0, Rgba([255, 0, 240, 255])),
            (65.0, Rgba([120, 0, 132, 255])),
            (70.0, Rgba([173, 144, 240, 255])),
        ];
        if value >= 70.0 {
            return Some(color_scale.last().unwrap().1);
        }
        for i in 1..color_scale.len() {
            let (low, low_color) = color_scale[i - 1];
            let (high, high_color) = color_scale[i];
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
