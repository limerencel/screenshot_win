# HDR Screenshot Win

An open-source, Rust-based, hardware-accelerated screenshot tool for Windows designed specifically to solve the severe "washed-out" and "overexposed" color issues when taking screenshots on HDR monitors.

## 🌟 The Problem

When using the built-in Windows Snipping Tool (`Win+Shift+S`) or third-party screenshot apps on an HDR monitor, ordinary SDR windows (like browsers, IDEs, and desktop UI) often appear horribly overexposed or washed out. 

This happens because generic tools either grab the HDR float buffer and apply an incorrect global tone-mapping curve (which crushes standard UI contrasts into pure white) or accidentally apply double gamma correction. 

## ✨ Features

This tool perfectly preserves your colors by grabbing the raw float buffers directly from the GPU and processing them with a custom color management workflow.

It provides two distinct modes:

- **SDR Mode (`Ctrl + Alt + A`)**: 
  Captures the screen, applies an intelligent tone mapping that **literally 1:1 maps** your SDR UI (so grays remain exact grays, whites remain exact whites), clips extreme HDR highlights cleanly, saves a standard `.png` file to your `Pictures\Screenshots` folder, AND seamlessly copies it to your clipboard for instant pasting.
- **Native HDR Mode (`Ctrl + Alt + H`)**: 
  Similar to Xbox Game Bar, this skips tone mapping and extracts the pure `R16G16B16A16` half-float raw data. Utilizing the Windows Imaging Component (WIC), it generates a true `.jxr` (JPEG XR) file that is natively supported by the Windows Photos app and retains the full dynamic range.

## 🚀 How to Use

1. **Build and Run**:
   Make sure you have the Rust `cargo` toolchain installed. Clone the repository and run:
   ```powershell
   cargo run --release
   ```
   The program will run silently in the console background waiting for hotkeys.

2. **Shortcuts**:
   - `Ctrl + Alt + A`: Capture screen, convert perfectly to SDR → Saves to `Pictures\Screenshots` & Copies to Clipboard.
   - `Ctrl + Alt + H`: Capture screen as True HDR → Saves to `Pictures\Screenshots` as a `.jxr` file.

3. **In the Overlay**:
   - Click and drag to select the region you want to capture.
   - Right-click or press `ESC` to cancel the capture.

## 🔬 Implementation Details & Technical Notes

Building this tool required bypassing the standard Windows API to solve color issues at the root:

1. **Raw DXGI Capture (No Windows Overlays)**
   We bypass standard BitBlt/PrintScreen. Instead, we use `IDXGIOutput5::DuplicateOutput1` to hook directly into the Desktop Window Manager (DWM). This gives us the raw `R16G16B16A16_FLOAT` linear scRGB buffer containing the absolute untouched pixel data.

2. **Intelligent SDR White Point Auto-Detection**
   Standard tone mapping algorithms (like ACES Filmic) apply a "soft shoulder" to compress high brightness. However, because Windows users adjust their "SDR Brightness Slider" to different values, these standard algorithms inadvertently compress standard light grays and UI whites into the same pure white ceiling, causing a washed-out look. 
   
   Our algorithm generates a rapid histogram of the captured float data. It dynamically locates the most frequent pure white/gray pixels above `1.0` to detect the exact "SDR White Level" your Windows slider is set to. It then applies a strict linear 1:1 mapping up to that detected white point, ensuring your IDEs and browsers look mathematically identical to what is on your screen.

3. **GDI Double Buffering**
   Creating a custom full-screen blackout overlay in Windows GDI usually results in severe flickering when dragging a rectangle. By explicitly disabling `WM_ERASEBKGND` and doing all composition (dimmed background + bright selection box) in a background memory Device Context (`CreateCompatibleDC`), we only execute a single `BitBlt` to the screen per frame.

4. **WIC (Windows Imaging Component) for JXR**
   Encoding `.jxr` files natively in pure Rust is extremely difficult. We interop directly with the Windows COM WIC API (`IWICImagingFactory`, `GUID_ContainerFormatWmp`), passing our 16-bit float array buffer to produce a format absolutely identical to Microsoft's native HDR dumps.
