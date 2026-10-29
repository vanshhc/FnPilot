//! Image comparison uses only in-memory pixels; screenshots never enter diagnostics.
use fnpilot_core::Result;
use std::io::Cursor;

pub(crate) fn desktop_point(
    pixel: (u32, u32),
    image: (u32, u32),
    bounds: (f64, f64, f64, f64),
) -> Result<(f64, f64)> {
    let (x, y) = pixel;
    let (width, height) = image;
    let (left, top, screen_width, screen_height) = bounds;
    if width == 0
        || height == 0
        || x >= width
        || y >= height
        || ![left, top, screen_width, screen_height]
            .iter()
            .all(|value| value.is_finite())
        || screen_width <= 0.0
        || screen_height <= 0.0
    {
        return Err("Invalid screenshot-to-desktop coordinates.".into());
    }
    Ok((
        left + x as f64 * screen_width / width as f64,
        top + y as f64 * screen_height / height as f64,
    ))
}

fn pixels(bytes: &[u8]) -> Result<(Vec<u8>, usize, usize, usize)> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|_| "Invalid captured PNG.")?;
    let size = reader
        .output_buffer_size()
        .filter(|size| *size <= 16_777_216)
        .ok_or("Screenshot is too large.")?;
    let mut data = vec![0; size];
    let info = reader
        .next_frame(&mut data)
        .map_err(|_| "Invalid captured pixels.")?;
    data.truncate(info.buffer_size());
    let channels = info.color_type.samples();
    if channels < 3 {
        return Err("Screenshot must contain RGB pixels.".into());
    }
    Ok((data, info.width as usize, info.height as usize, channels))
}

/// Ignore tiny animation noise; stop when the click region (or keyboard scene) changes.
pub(crate) fn unchanged(before: &[u8], after: &[u8], point: Option<(u32, u32)>) -> Result<bool> {
    let (a, width, height, ac) = pixels(before)?;
    let (b, bw, bh, bc) = pixels(after)?;
    if width != bw || height != bh {
        return Ok(false);
    }
    let (left, top, right, bottom, stride) = if let Some((x, y)) = point {
        let (x, y) = (x as usize, y as usize);
        (
            x.saturating_sub(48),
            y.saturating_sub(48),
            (x + 49).min(width),
            (y + 49).min(height),
            1,
        )
    } else {
        (0, 0, width, height, 4)
    };
    let (mut changed, mut count) = (0, 0);
    for y in (top..bottom).step_by(stride) {
        for x in (left..right).step_by(stride) {
            let ai = (y * width + x) * ac;
            let bi = (y * width + x) * bc;
            changed += usize::from((0..3).any(|c| a[ai + c].abs_diff(b[bi + c]) > 24));
            count += 1;
        }
    }
    Ok(count > 0 && changed * 100 <= count * 2)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resized_screenshots_map_to_retina_and_negative_monitor_origins() {
        assert_eq!(
            desktop_point((640, 400), (1280, 800), (-1920.0, 30.0, 3840.0, 2400.0)).unwrap(),
            (0.0, 1230.0)
        );
        assert_eq!(
            desktop_point((0, 0), (1280, 800), (-1920.0, 30.0, 3840.0, 2400.0)).unwrap(),
            (-1920.0, 30.0)
        );
        assert!(desktop_point((1280, 0), (1280, 800), (0.0, 0.0, 3840.0, 2400.0)).is_err());
    }
    fn png(changed: bool) -> Vec<u8> {
        let mut data = vec![0u8; 128 * 128 * 3];
        if changed {
            for y in 40..80 {
                for x in 40..80 {
                    data[(y * 128 + x) * 3] = 255;
                }
            }
        }
        let mut bytes = vec![];
        {
            let mut encoder = png::Encoder::new(&mut bytes, 128, 128);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&data)
                .unwrap();
        }
        bytes
    }
    #[test]
    fn stale_click_regions_and_keyboard_scenes_stop_input() {
        let before = png(false);
        let after = png(true);
        assert!(unchanged(&before, &before, Some((60, 60))).unwrap());
        assert!(!unchanged(&before, &after, Some((60, 60))).unwrap());
        assert!(!unchanged(&before, &after, None).unwrap());
        assert!(unchanged(b"invalid", &after, None).is_err());
    }
}
