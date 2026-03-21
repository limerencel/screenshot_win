// Removed tonemap_channel completely, relying on dynamic sdr_white inside the loop.
/// Applies the standard sRGB transfer function.
#[inline]
fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Tone-map HDR float pixels (RGBA f32) to SDR u8 pixels (RGBA).
///
/// Pipeline: scRGB linear → ACES tone map → sRGB gamma → quantize to 0..255
pub fn tonemap_to_srgb(pixels: &[f32], width: u32, height: u32, is_hdr: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity((width * height * 4) as usize);
    let mut sdr_white_level = 2.0_f32;

    if is_hdr {
        // Auto-detect SDR white level by finding the most common pure gray/white pixel > 1.0
        let mut buckets = std::collections::HashMap::new();
        for i in (0..pixels.len()).step_by(4) {
            let r = pixels[i];
            let g = pixels[i+1];
            let b = pixels[i+2];
            // Only examine pixels that are completely gray/white
            if r >= 1.0 && (r - g).abs() < 0.005 && (r - b).abs() < 0.005 {
                let bucket = (r * 100.0).round() as i32; 
                *buckets.entry(bucket).or_insert(0) += 1;
            }
        }
        
        let mut max_count = 0;
        for (bucket, count) in buckets {
            if count > max_count && count > 50 { // min threshold to trust the bucket
                max_count = count;
                sdr_white_level = (bucket as f32) / 100.0;
            }
        }
        sdr_white_level = sdr_white_level.clamp(1.0, 10.0);
        println!("   [Color] Auto-detected SDR White Level: {}", sdr_white_level);
    }

    for i in 0..(width * height) as usize {
        let base = i * 4;
        let (r, g, b) = if is_hdr {
            // HDR path: strict linear map to SDR white, no shoulder compression
            // This perfectly guarantees exact same UI colors (light grays don't become white)
            let r = linear_to_srgb((pixels[base] / sdr_white_level).clamp(0.0, 1.0));
            let g = linear_to_srgb((pixels[base + 1] / sdr_white_level).clamp(0.0, 1.0));
            let b = linear_to_srgb((pixels[base + 2] / sdr_white_level).clamp(0.0, 1.0));
            (r, g, b)
        } else {
            // SDR path: data is already in [0,1] sRGB, pass through
            (pixels[base], pixels[base + 1], pixels[base + 2])
        };

        out.push((r * 255.0).round().clamp(0.0, 255.0) as u8);
        out.push((g * 255.0).round().clamp(0.0, 255.0) as u8);
        out.push((b * 255.0).round().clamp(0.0, 255.0) as u8);
        out.push(255u8); // Alpha: always fully opaque for screenshot
    }

    out
}
