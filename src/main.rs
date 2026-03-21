mod capture;
mod clipboard;
mod overlay;
mod tonemap;
mod save;

use windows::Win32::Foundation::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use std::env;
use std::fs;
use std::path::PathBuf;
use chrono::Local;

const HOTKEY_SDR_ID: i32 = 1;
const HOTKEY_HDR_ID: i32 = 2;

fn get_screenshots_folder() -> PathBuf {
    let mut dir = PathBuf::from(env::var("USERPROFILE").unwrap_or_else(|_| "C:\\".to_string()));
    dir.push("Pictures");
    dir.push("Screenshots");
    if !dir.exists() {
        let _ = fs::create_dir_all(&dir);
    }
    dir
}

fn take_screenshot(hdr_file_mode: bool) {
    println!("\n📸 Capturing...");

    // 1. Silent DXGI capture (no overlay on screen yet)
    let frame = match capture::capture_screen() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("❌ Capture failed: {e}");
            return;
        }
    };
    println!("   {}x{} {}", frame.width, frame.height, if frame.is_hdr { "HDR" } else { "SDR" });

    // 2. Tone-map entire frame to sRGB (needed for overlay display)
    let srgb = tonemap::tonemap_to_srgb(&frame.pixels, frame.width, frame.height, frame.is_hdr);

    // 3. Show selection overlay on the tone-mapped image
    let sel = overlay::show_selection(&srgb, frame.width, frame.height);

    match sel {
        Some(rect) => {
            println!("   Selection: {}x{} at ({},{})", rect.w, rect.h, rect.x, rect.y);

            let folder = get_screenshots_folder();
            let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();

            if hdr_file_mode {
                let cropped_f32 = crop_f32(&frame.pixels, frame.width, &rect);
                let mut path = folder.clone();
                path.push(format!("Screenshot_HDR_{}.jxr", timestamp));
                let path_str = path.to_str().unwrap();

                match save::save_as_jxr(path_str, rect.w, rect.h, &cropped_f32) {
                    Ok(()) => println!("✅ Saved HDR screenshot to {}", path_str),
                    Err(e) => eprintln!("❌ HDR save error: {:?}", e),
                }
            } else {
                // 4. Crop selected region (SDR)
                let cropped = crop_rgba(&srgb, frame.width, &rect);

                let mut path = folder.clone();
                path.push(format!("Screenshot_{}.png", timestamp));
                let path_str = path.to_str().unwrap();

                // Save to file
                match image::save_buffer(path_str, &cropped, rect.w, rect.h, image::ColorType::Rgba8) {
                    Ok(()) => println!("✅ Saved SDR screenshot to {}", path_str),
                    Err(e) => eprintln!("❌ SDR save error: {:?}", e),
                }

                // 5. Copy to clipboard
                match clipboard::copy_to_clipboard(&cropped, rect.w, rect.h) {
                    Ok(()) => println!("✅ Copied to clipboard! Ctrl+V to paste"),
                    Err(e) => eprintln!("❌ Clipboard error: {e}"),
                }
            }
        }
        None => {
            println!("❎ Cancelled");
        }
    }
}

fn crop_f32(pixels: &[f32], full_width: u32, rect: &overlay::SelectionRect) -> Vec<f32> {
    let mut out = Vec::with_capacity((rect.w * rect.h * 4) as usize);
    for y in rect.y..(rect.y + rect.h) {
        let row_start = (y * full_width + rect.x) as usize * 4;
        let row_end = row_start + (rect.w as usize * 4);
        out.extend_from_slice(&pixels[row_start..row_end]);
    }
    out
}

fn crop_rgba(pixels: &[u8], full_width: u32, rect: &overlay::SelectionRect) -> Vec<u8> {
    let mut out = Vec::with_capacity((rect.w * rect.h * 4) as usize);
    for y in rect.y..(rect.y + rect.h) {
        let row_start = (y * full_width + rect.x) as usize * 4;
        let row_end = row_start + (rect.w as usize * 4);
        out.extend_from_slice(&pixels[row_start..row_end]);
    }
    out
}

fn main() {
    println!("╔══════════════════════════════════════════╗");
    println!("║   HDR Screenshot Tool                    ║");
    println!("║   Press [Ctrl+Alt+A] to copy SDR to Clip ║");
    println!("║   Press [Ctrl+Alt+H] to save HDR to JXR  ║");
    println!("║   Drag to select region                  ║");
    println!("║   Right-click or ESC to cancel           ║");
    println!("║   Ctrl+C to quit                         ║");
    println!("╚══════════════════════════════════════════╝");

    unsafe {
        let mods = HOT_KEY_MODIFIERS(MOD_CONTROL.0 | MOD_ALT.0 | MOD_NOREPEAT.0);
        let res1 = RegisterHotKey(HWND::default(), HOTKEY_SDR_ID, mods, 0x41); // 'A'
        let res2 = RegisterHotKey(HWND::default(), HOTKEY_HDR_ID, mods, 0x48); // 'H'

        if res1.is_err() || res2.is_err() {
            eprintln!("❌ Failed to register hotkeys.");
            eprintln!("   Close other screenshot tools and retry.");
            return;
        }

        println!("\n🎯 Waiting for hotkeys...\n");

        let mut msg = MSG::default();
        loop {
            let ret = GetMessageW(&mut msg, HWND::default(), 0, 0);
            if ret.0 <= 0 {
                break;
            }
            if msg.message == WM_HOTKEY {
                if msg.wParam.0 == HOTKEY_SDR_ID as usize {
                    take_screenshot(false);
                } else if msg.wParam.0 == HOTKEY_HDR_ID as usize {
                    take_screenshot(true);
                }
                println!("\n🎯 Waiting for hotkeys...\n");
            }
        }

        let _ = UnregisterHotKey(HWND::default(), HOTKEY_SDR_ID);
        let _ = UnregisterHotKey(HWND::default(), HOTKEY_HDR_ID);
    }
}
