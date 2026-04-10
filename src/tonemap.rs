#[derive(Clone, Copy)]
pub struct TonemapSettings {
    pub reference_white: f32,
}

pub const PREVIEW_SETTINGS: TonemapSettings = TonemapSettings {
    reference_white: 1.0,
};

pub const EXPORT_SETTINGS: TonemapSettings = TonemapSettings {
    reference_white: 1.0,
};

#[inline]
fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

#[inline]
fn luminance(rgb: [f32; 3]) -> f32 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

#[inline]
fn aces_film(x: f32) -> f32 {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    ((x * (a * x + b)) / (x * (c * x + d) + e)).clamp(0.0, 1.0)
}

fn tonemap_hdr_rgb(rgb: [f32; 3], settings: TonemapSettings) -> [f32; 3] {
    // Normalize by reference_white (SDR boost level) to get "normal" SDR brightness
    // This preserves what user sees: boosted SDR on HDR monitor → normal SDR in screenshot
    let normalized = [
        rgb[0].max(0.0) / settings.reference_white,
        rgb[1].max(0.0) / settings.reference_white,
        rgb[2].max(0.0) / settings.reference_white,
    ];

    let scene_luma = luminance(normalized);
    if scene_luma <= f32::EPSILON {
        return [0.0, 0.0, 0.0];
    }

    // Only compress highlights (values > 1.0 after normalization, i.e., true HDR)
    // For SDR content (luma <= 1.0), pass through unchanged to preserve exact appearance
    let mapped = if scene_luma > 1.0 {
        // Apply ACES tone mapping for HDR highlights
        let mapped_luma = aces_film(scene_luma);
        let luma_scale = mapped_luma / scene_luma;
        let mut compressed = [
            normalized[0] * luma_scale,
            normalized[1] * luma_scale,
            normalized[2] * luma_scale,
        ];

        // Keep saturated highlights inside sRGB gamut
        let peak = compressed[0].max(compressed[1]).max(compressed[2]);
        if peak > 1.0 {
            let gamut_scale = 1.0 / peak;
            compressed[0] *= gamut_scale;
            compressed[1] *= gamut_scale;
            compressed[2] *= gamut_scale;
        }
        compressed
    } else {
        // SDR content: pass through unchanged (preserve exact visual match)
        normalized
    };

    mapped
}

pub fn tonemap_to_srgb(
    pixels: &[f32],
    width: u32,
    height: u32,
    is_hdr: bool,
    settings: TonemapSettings,
) -> Vec<u8> {
    let mut out = Vec::with_capacity((width * height * 4) as usize);

    if is_hdr {
        println!(
            "   [Color] HDR->SDR tone mapping (preserve visual appearance)"
        );
    } else {
        println!(
            "   [Color] SDR capture (direct sRGB, no gamma conversion)"
        );
    }

    for i in 0..(width * height) as usize {
        let base = i * 4;
        
        if is_hdr {
            // HDR input is linear scRGB (R16G16B16A16_FLOAT)
            // Apply tone mapping to compress HDR range, then convert to sRGB
            let rgb = tonemap_hdr_rgb([pixels[base], pixels[base + 1], pixels[base + 2]], settings);
            out.push((linear_to_srgb(rgb[0]).clamp(0.0, 1.0) * 255.0).round() as u8);
            out.push((linear_to_srgb(rgb[1]).clamp(0.0, 1.0) * 255.0).round() as u8);
            out.push((linear_to_srgb(rgb[2]).clamp(0.0, 1.0) * 255.0).round() as u8);
        } else {
            // SDR input (B8G8R8A8_UNORM) is already sRGB encoded
            // DO NOT apply linear_to_srgb - just output directly
            out.push((pixels[base].clamp(0.0, 1.0) * 255.0).round() as u8);
            out.push((pixels[base + 1].clamp(0.0, 1.0) * 255.0).round() as u8);
            out.push((pixels[base + 2].clamp(0.0, 1.0) * 255.0).round() as u8);
        }
        out.push(255u8);
    }

    out
}
