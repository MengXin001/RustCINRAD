//! Viewport-local drawing without Plotters' per-vertex coordinate truncation.

use crate::RadarError::Output;
use crate::Result;
use plotters::prelude::*;

pub(super) fn path(
    canvas: &mut BitMapBackend<'_>,
    bounds: (f64, f64, f64, f64),
    points: impl IntoIterator<Item = (f64, f64)>,
    color: RGBColor,
) -> Result<()> {
    let (west, east, south, north) = bounds;
    let (width, height) = canvas.get_size();
    let pixels = points.into_iter().map(|(x, y)| {
        (
            ((x - west) / (east - west) * width as f64).round() as i32,
            ((north - y) / (north - south) * height as f64).round() as i32,
        )
    });
    // Only the backend's pixel bounds limit output. No source vertices are moved,
    // simplified, clipped, joined, or replaced with frame coordinates.
    canvas.draw_path(pixels, &color.stroke_width(1)).map_err(|e| Output(e.to_string()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {

    use super::*;

    #[test]
    fn crossing_retains_slope_and_outside_path_never_draws_frame_line() {
        let mut rgb = vec![0; 100 * 100 * 3];
        {
            let mut canvas = BitMapBackend::with_buffer(&mut rgb, (100, 100));
            path(&mut canvas, (0.0, 10.0, 0.0, 10.0), [(-5.0, 0.0), (15.0, 10.0)], WHITE).unwrap();
            path(&mut canvas, (0.0, 10.0, 0.0, 10.0), [(-5.0, 3.0), (-1.0, 9.0)], WHITE).unwrap();
        }
        let pixel = |x: usize, y: usize| &rgb[(y * 100 + x) * 3..(y * 100 + x + 1) * 3];
        assert!(pixel(0, 75).iter().any(|&v| v > 0));
        assert_eq!(pixel(0, 95), [0, 0, 0]);
        assert_eq!(pixel(0, 30), [0, 0, 0]);
    }

    #[test]
    fn outside_excursion_does_not_connect_at_border() {
        let mut rgb = vec![0; 100 * 100 * 3];
        {
            let mut canvas = BitMapBackend::with_buffer(&mut rgb, (100, 100));
            path(&mut canvas, (0.0, 10.0, 0.0, 10.0), [(2.0, 2.0), (-2.0, 4.0), (-2.0, 8.0), (2.0, 8.0)], WHITE)
                .unwrap();
        }
        assert_eq!(&rgb[40 * 100 * 3..(40 * 100 + 1) * 3], [0, 0, 0]);
    }
}
