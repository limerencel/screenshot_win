use windows::core::*;
use std::path::Path;
use windows::Win32::System::Com::*;
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::Foundation::GENERIC_WRITE;
use std::ffi::c_void;
use half::f16;

pub fn save_as_jxr(path: &str, width: u32, height: u32, float_pixels: &[f32]) -> Result<()> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED); // It's fine if already initialized

        let factory: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;

        let encoder = factory.CreateEncoder(&GUID_ContainerFormatWmp, std::ptr::null())?;

        let stream = factory.CreateStream()?;
        let path_u16: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        stream.InitializeFromFilename(PCWSTR(path_u16.as_ptr()), GENERIC_WRITE.0)?;

        encoder.Initialize(&stream, WICBitmapEncoderNoCache)?;

        let mut frame: Option<IWICBitmapFrameEncode> = None;
        let mut props: Option<windows::Win32::System::Com::StructuredStorage::IPropertyBag2> = None;
        encoder.CreateNewFrame(&mut frame, &mut props)?;
        let frame = frame.unwrap();

        frame.Initialize(props.as_ref())?;
        frame.SetSize(width, height)?;

        let mut format = GUID_WICPixelFormat64bppRGBAHalf; // WIC supports half float
        frame.SetPixelFormat(&mut format)?;
        
        // Convert f32 back to f16 (what WIC expects for Half format)
        let mut f16_pixels = Vec::with_capacity(float_pixels.len());
        for &p in float_pixels {
            f16_pixels.push(f16::from_f32(p).to_bits());
        }

        let stride = width * 8; // 4 channels x 2 bytes
        let size = (f16_pixels.len() * 2) as u32;

        let data = std::slice::from_raw_parts(f16_pixels.as_ptr() as *const u8, size as usize);
        frame.WritePixels(height, stride, data)?;

        frame.Commit()?;
        encoder.Commit()?;

        Ok(())
    }
}
