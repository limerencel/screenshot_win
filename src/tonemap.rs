#[derive(Clone, Copy)]
pub struct TonemapSettings {
    pub reference_white: f32,
    pub exposure: f32,
}

pub const PREVIEW_SETTINGS: TonemapSettings = TonemapSettings {
    reference_white: 2.0,
    exposure: 0.95,
};

pub const EXPORT_SETTINGS: TonemapSettings = TonemapSettings {
    reference_white: 2.0,
    exposure: 0.90,
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
    let scaled = [
        (rgb[0].max(0.0) / settings.reference_white) * settings.exposure,
        (rgb[1].max(0.0) / settings.reference_white) * settings.exposure,
        (rgb[2].max(0.0) / settings.reference_white) * settings.exposure,
    ];

    let scene_luma = luminance(scaled);
    if scene_luma <= f32::EPSILON {
        return [0.0, 0.0, 0.0];
    }

    // Compress highlights based on brightness, then rescale RGB to preserve hue.
    let mapped_luma = aces_film(scene_luma);
    let luma_scale = mapped_luma / scene_luma;
    let mut mapped = [
        scaled[0] * luma_scale,
        scaled[1] * luma_scale,
        scaled[2] * luma_scale,
    ];

    // Keep saturated highlights inside sRGB gamut instead of clipping to white.
    let peak = mapped[0].max(mapped[1]).max(mapped[2]);
    if peak > 1.0 {
        let gamut_scale = 1.0 / peak;
        mapped[0] *= gamut_scale;
        mapped[1] *= gamut_scale;
        mapped[2] *= gamut_scale;
    }

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
            "   [Color] HDR->SDR reference white {:.2}, exposure {:.2}",
            settings.reference_white, settings.exposure
        );
    }

    for i in 0..(width * height) as usize {
        let base = i * 4;
        let rgb = if is_hdr {
            tonemap_hdr_rgb([pixels[base], pixels[base + 1], pixels[base + 2]], settings)
        } else {
            [
                pixels[base].clamp(0.0, 1.0),
                pixels[base + 1].clamp(0.0, 1.0),
                pixels[base + 2].clamp(0.0, 1.0),
            ]
        };

        out.push((linear_to_srgb(rgb[0]).clamp(0.0, 1.0) * 255.0).round() as u8);
        out.push((linear_to_srgb(rgb[1]).clamp(0.0, 1.0) * 255.0).round() as u8);
        out.push((linear_to_srgb(rgb[2]).clamp(0.0, 1.0) * 255.0).round() as u8);
        out.push(255u8);
    }

    out
}
