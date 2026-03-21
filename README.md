# HDR Screenshot Win

An open-source, Rust-based screenshot tool for Windows focused on producing usable SDR and native HDR screenshots on HDR monitors without the severe washout common in generic capture tools.

## The Problem

On an HDR desktop, many screenshot tools either:

- capture the HDR desktop buffer and map it to SDR incorrectly, causing overexposed whites and blown-out colors
- or use APIs that do not preserve the actual HDR desktop data

That is why ordinary SDR UI such as browsers, IDEs, and desktop windows can look much brighter in screenshots than they do on screen.

## Features

This project provides two capture modes:

- **SDR Mode (`Ctrl + Alt + A`)**  
  Captures the selected region, converts HDR desktop data to SDR PNG, saves it to `Pictures\Screenshots`, and copies it to the clipboard.
- **Native HDR Mode (`Ctrl + Alt + H`)**  
  Saves the selected region as a `.jxr` file using the native `R16G16B16A16_FLOAT` desktop data path so the HDR range is preserved.

The current SDR path is designed to avoid the worst failure modes first:

- use the system SDR white level instead of guessing it from image contents
- apply highlight roll-off to reduce overexposed bright regions
- compress out-of-range highlights more gracefully so saturated colors are less likely to clip to white

It is closer to the Windows HDR desktop behavior than a naive global scale, but it is still not identical to Xbox Game Bar.

## How to Use

1. **Build and run**
   Make sure Rust and `cargo` are installed, then run:

   ```powershell
   cargo run --release
   ```

2. **Shortcuts**
   - `Ctrl + Alt + A`: Capture a region and export SDR PNG
   - `Ctrl + Alt + H`: Capture a region and export native HDR JXR

3. **Overlay controls**
   - Drag to select a region
   - Right-click or press `Esc` to cancel

## Implementation Notes

1. **Raw DXGI desktop capture**  
   The app uses `IDXGIOutput5::DuplicateOutput1` when available to read the desktop duplication stream directly. On HDR desktops, that can expose the scRGB `R16G16B16A16_FLOAT` surface instead of an already-tonemapped SDR image.

2. **System SDR white level**  
   For SDR export on HDR desktops, the app queries Windows for the current SDR white level with `QueryDisplayConfig` and `DISPLAYCONFIG_SDR_WHITE_LEVEL`. This aligns the SDR conversion with the display's current HDR configuration instead of inferring white from the screenshot contents.

3. **HDR to SDR mapping**  
   SDR export uses a brightness-based shoulder curve plus simple gamut compression. The goal is to keep SDR UI readable and reduce highlight blowout, not to claim exact pixel-perfect reproduction against the Windows compositor.

4. **Overlay rendering**  
   The selection overlay is rendered with GDI double buffering to avoid excessive flicker while dragging. The preview image and the final SDR export are generated separately so overlay dimming does not affect the saved file.

5. **JXR export**  
   HDR files are encoded through Windows Imaging Component (WIC) using `GUID_ContainerFormatWmp`, which allows saving half-float image data in a format the Windows Photos app can open.

## Current Limitations

- SDR output is improved, but still not a perfect match for Xbox Game Bar in every scene.
- Multi-monitor handling still assumes the duplicated output and the selected region belong to the same display path.
- The SDR conversion is tuned to avoid obvious clipping first; further refinement is still possible for difficult HDR scenes.
