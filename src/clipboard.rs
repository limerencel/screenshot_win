use windows::core::Result;
use windows::Win32::Foundation::*;
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::Memory::*;

/// CF_DIB clipboard format value
const CF_DIB_VALUE: u32 = 8;

/// Copy RGBA u8 pixel data to the Windows clipboard as a CF_DIB bitmap.
/// This lets you Ctrl+V into any application (WeChat, Paint, Word, etc.)
pub fn copy_to_clipboard(pixels: &[u8], width: u32, height: u32) -> Result<()> {
    unsafe {
        // Build a DIB (Device Independent Bitmap) in memory
        // CF_DIB expects: BITMAPINFOHEADER followed by pixel data (bottom-up BGR)
        let header_size = 40usize; // sizeof(BITMAPINFOHEADER) = 40 bytes
        let row_stride = ((width * 3 + 3) & !3) as usize; // 3 bytes per pixel (BGR), padded to 4-byte boundary
        let pixel_data_size = row_stride * height as usize;
        let total_size = header_size + pixel_data_size;

        // Allocate global memory for clipboard
        let hmem = GlobalAlloc(GMEM_MOVEABLE, total_size)?;
        let ptr = GlobalLock(hmem) as *mut u8;
        if ptr.is_null() {
            let _ = GlobalFree(hmem);
            return Err(windows::core::Error::new(E_FAIL, "GlobalLock failed"));
        }

        // Write BITMAPINFOHEADER manually (40 bytes)
        let header = ptr as *mut [u8; 40];
        let mut hdr = [0u8; 40];
        // biSize = 40
        hdr[0..4].copy_from_slice(&40u32.to_le_bytes());
        // biWidth
        hdr[4..8].copy_from_slice(&(width as i32).to_le_bytes());
        // biHeight (positive = bottom-up)
        hdr[8..12].copy_from_slice(&(height as i32).to_le_bytes());
        // biPlanes = 1
        hdr[12..14].copy_from_slice(&1u16.to_le_bytes());
        // biBitCount = 24
        hdr[14..16].copy_from_slice(&24u16.to_le_bytes());
        // biCompression = 0 (BI_RGB)
        hdr[16..20].copy_from_slice(&0u32.to_le_bytes());
        // biSizeImage
        hdr[20..24].copy_from_slice(&(pixel_data_size as u32).to_le_bytes());
        // Rest are zeros (already zeroed)
        *header = hdr;

        // Write pixel data (convert RGBA top-down to BGR bottom-up)
        let dest = ptr.add(header_size);
        for y in 0..height {
            let src_row = (height - 1 - y) as usize; // flip vertically
            for x in 0..width {
                let src_idx = (src_row * width as usize + x as usize) * 4;
                let dst_idx = y as usize * row_stride + x as usize * 3;
                // RGBA → BGR
                *dest.add(dst_idx) = pixels[src_idx + 2]; // B
                *dest.add(dst_idx + 1) = pixels[src_idx + 1]; // G
                *dest.add(dst_idx + 2) = pixels[src_idx]; // R
            }
        }

        let _ = GlobalUnlock(hmem);

        // Set clipboard data
        OpenClipboard(HWND::default())?;
        EmptyClipboard()?;

        let result = SetClipboardData(CF_DIB_VALUE, HANDLE(hmem.0));
        let _ = CloseClipboard();

        if result.is_err() {
            return Err(windows::core::Error::new(E_FAIL, "SetClipboardData failed"));
        }

        Ok(())
    }
}
