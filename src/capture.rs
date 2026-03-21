use windows::core::{Interface, Result};
use windows::Win32::Devices::Display::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;

pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<f32>,
    pub is_hdr: bool,
    pub sdr_white_level: Option<f32>,
}

pub fn capture_screen() -> Result<CapturedFrame> {
    unsafe {
        let mut device: Option<ID3D11Device> = None;
        let mut context: Option<ID3D11DeviceContext> = None;
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&[D3D_FEATURE_LEVEL_11_0]),
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )?;
        let device = device.unwrap();
        let context = context.unwrap();

        let dxgi_device: IDXGIDevice = device.cast()?;
        let adapter: IDXGIAdapter = dxgi_device.GetAdapter()?;
        let output: IDXGIOutput = adapter.EnumOutputs(0)?;

        let (duplication, is_hdr) = create_duplication(&output, &device)?;
        let sdr_white_level = query_sdr_white_level(&output);

        // Acquire frame
        let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource: Option<IDXGIResource> = None;
        for attempt in 0..10 {
            match duplication.AcquireNextFrame(500, &mut frame_info, &mut resource) {
                Ok(()) => {
                    if frame_info.LastPresentTime != 0 || attempt >= 2 {
                        break;
                    }
                    duplication.ReleaseFrame().ok();
                    resource = None;
                }
                Err(e) => {
                    if attempt < 9 {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        continue;
                    }
                    return Err(e);
                }
            }
        }
        let resource =
            resource.ok_or_else(|| windows::core::Error::new(E_FAIL, "No frame acquired"))?;

        let texture: ID3D11Texture2D = resource.cast()?;
        let mut tex_desc = D3D11_TEXTURE2D_DESC::default();
        texture.GetDesc(&mut tex_desc);
        let width = tex_desc.Width;
        let height = tex_desc.Height;

        // Staging texture for CPU read
        let staging_desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: tex_desc.Format,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };
        let mut staging: Option<ID3D11Texture2D> = None;
        device.CreateTexture2D(&staging_desc, None, Some(&mut staging))?;
        let staging = staging.unwrap();

        // Copy and map
        let src_res: ID3D11Resource = texture.cast()?;
        let dst_res: ID3D11Resource = staging.cast()?;
        context.CopyResource(&dst_res, &src_res);

        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        context.Map(&dst_res, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;

        let pixels = if is_hdr {
            read_hdr(mapped.pData as *const u8, mapped.RowPitch, width, height)
        } else {
            read_sdr(mapped.pData as *const u8, mapped.RowPitch, width, height)
        };

        context.Unmap(&dst_res, 0);
        duplication.ReleaseFrame().ok();

        Ok(CapturedFrame { width, height, pixels, is_hdr, sdr_white_level })
    }
}

unsafe fn create_duplication(
    output: &IDXGIOutput,
    device: &ID3D11Device,
) -> Result<(IDXGIOutputDuplication, bool)> {
    if let Ok(output5) = output.cast::<IDXGIOutput5>() {
        let formats = [DXGI_FORMAT_R16G16B16A16_FLOAT];
        if let Ok(dup) = output5.DuplicateOutput1(device, 0, &formats) {
            let desc = dup.GetDesc();
            if desc.ModeDesc.Format == DXGI_FORMAT_R16G16B16A16_FLOAT {
                println!("[HDR] R16G16B16A16_FLOAT");
                return Ok((dup, true));
            }
            println!("[SDR] Non-HDR format from DuplicateOutput1");
            return Ok((dup, false));
        }
    }
    let output1: IDXGIOutput1 = output.cast()?;
    let dup = output1.DuplicateOutput(device)?;
    println!("[SDR] B8G8R8A8_UNORM fallback");
    Ok((dup, false))
}

fn read_hdr(data: *const u8, pitch: u32, w: u32, h: u32) -> Vec<f32> {
    use half::f16;
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let off = (y * pitch) as isize + (x as isize * 8);
            let ptr = unsafe { data.offset(off) as *const u16 };
            let raw = unsafe { std::slice::from_raw_parts(ptr, 4) };
            out.push(f16::from_bits(raw[0]).to_f32());
            out.push(f16::from_bits(raw[1]).to_f32());
            out.push(f16::from_bits(raw[2]).to_f32());
            out.push(f16::from_bits(raw[3]).to_f32());
        }
    }
    out
}

fn read_sdr(data: *const u8, pitch: u32, w: u32, h: u32) -> Vec<f32> {
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let off = (y * pitch) as isize + (x as isize * 4);
            let ptr = unsafe { data.offset(off) };
            let bgra = unsafe { std::slice::from_raw_parts(ptr, 4) };
            out.push(bgra[2] as f32 / 255.0);
            out.push(bgra[1] as f32 / 255.0);
            out.push(bgra[0] as f32 / 255.0);
            out.push(bgra[3] as f32 / 255.0);
        }
    }
    out
}

fn query_sdr_white_level(output: &IDXGIOutput) -> Option<f32> {
    unsafe {
        let output_desc = output.GetDesc().ok()?;
        let gdi_name = utf16_to_string(&output_desc.DeviceName);
        if gdi_name.is_empty() {
            return None;
        }

        let path = find_display_path_for_gdi_name(&gdi_name)?;
        let mut request = DISPLAYCONFIG_SDR_WHITE_LEVEL {
            header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL,
                size: std::mem::size_of::<DISPLAYCONFIG_SDR_WHITE_LEVEL>() as u32,
                adapterId: path.targetInfo.adapterId,
                id: path.targetInfo.id,
            },
            ..Default::default()
        };

        let status = DisplayConfigGetDeviceInfo(&mut request.header);
        if status != 0 {
            eprintln!(
                "   [Color] Failed to query SDR white level for {} (error {})",
                gdi_name, status
            );
            return None;
        }

        let reference_white = request.SDRWhiteLevel as f32 / 1000.0;
        let white_nits = reference_white * 80.0;
        println!(
            "   [Color] System SDR white for {}: {:.1} nits ({:.3}x)",
            gdi_name, white_nits, reference_white
        );
        Some(reference_white.max(1.0))
    }
}

fn find_display_path_for_gdi_name(gdi_name: &str) -> Option<DISPLAYCONFIG_PATH_INFO> {
    unsafe {
        for _ in 0..3 {
            let mut path_count = 0;
            let mut mode_count = 0;
            let status = GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count);
            if status != ERROR_SUCCESS {
                return None;
            }

            let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
            let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
            let status = QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut path_count,
                paths.as_mut_ptr(),
                &mut mode_count,
                modes.as_mut_ptr(),
                None,
            );

            if status == ERROR_INSUFFICIENT_BUFFER {
                continue;
            }
            if status != ERROR_SUCCESS {
                return None;
            }

            paths.truncate(path_count as usize);
            for path in paths {
                if source_device_name(&path) == Some(gdi_name.to_string()) {
                    return Some(path);
                }
            }
            return None;
        }
        None
    }
}

fn source_device_name(path: &DISPLAYCONFIG_PATH_INFO) -> Option<String> {
    unsafe {
        let mut request = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
            header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                size: std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                adapterId: path.sourceInfo.adapterId,
                id: path.sourceInfo.id,
            },
            ..Default::default()
        };

        let status = DisplayConfigGetDeviceInfo(&mut request.header);
        if status != 0 {
            return None;
        }
        Some(utf16_to_string(&request.viewGdiDeviceName))
    }
}

fn utf16_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}
