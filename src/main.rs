mod capture;
mod clipboard;
mod overlay;
mod tonemap;
mod save;

use windows::Win32::Foundation::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::System::Threading::GetCurrentThreadId;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use chrono::Local;

const HOTKEY_SDR_ID: i32 = 1;
const HOTKEY_HDR_ID: i32 = 2;

static MAIN_THREAD_ID: AtomicU32 = AtomicU32::new(0);
const WM_USER_TAKE_SCREENSHOT: u32 = WM_USER + 1;

unsafe extern "system" fn low_level_keyboard_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code >= 0 {
        let kbd = *(lparam.0 as *const KBDLLHOOKSTRUCT);
        if kbd.vkCode == VK_SNAPSHOT.0 as u32 {
            let event = wparam.0 as u32;
            if event == WM_KEYDOWN || event == WM_SYSKEYDOWN {
                let thread_id = MAIN_THREAD_ID.load(Ordering::SeqCst);
                if thread_id != 0 {
                    let _ = PostThreadMessageW(thread_id, WM_USER_TAKE_SCREENSHOT, WPARAM(0), LPARAM(0));
                }
            }
            return LRESULT(1); // Swallow Print Screen keypress
        }
    }
    CallNextHookEx(HHOOK::default(), code, wparam, lparam)
}

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

    let preview_settings = capture_settings(&frame, tonemap::PREVIEW_SETTINGS);
    let export_settings = capture_settings(&frame, tonemap::EXPORT_SETTINGS);

    // 2. Build a preview image for the selection overlay.
    let preview = tonemap::tonemap_to_srgb(
        &frame.pixels,
        frame.width,
        frame.height,
        frame.is_hdr,
        preview_settings,
    );

    // 3. Show selection overlay on the preview image.
    let sel = overlay::show_selection(&preview, frame.width, frame.height);

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
                // 4. Re-run the SDR conversion on the selected raw pixels.
                let cropped_f32 = crop_f32(&frame.pixels, frame.width, &rect);
                let cropped = tonemap::tonemap_to_srgb(
                    &cropped_f32,
                    rect.w,
                    rect.h,
                    frame.is_hdr,
                    export_settings,
                );

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

fn take_direct_screenshot() {
    println!("\n📸 Direct Fullscreen Capturing...");

    // 1. Silent DXGI capture (no overlay, captures immediately)
    let frame = match capture::capture_screen() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("❌ Capture failed: {e}");
            return;
        }
    };
    println!("   {}x{} {}", frame.width, frame.height, if frame.is_hdr { "HDR" } else { "SDR" });

    let folder = get_screenshots_folder();
    let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();

    // 2. We always save the SDR version as a PNG file and copy to clipboard
    let export_settings = capture_settings(&frame, tonemap::EXPORT_SETTINGS);
    let sdr_pixels = tonemap::tonemap_to_srgb(
        &frame.pixels,
        frame.width,
        frame.height,
        frame.is_hdr,
        export_settings,
    );

    let mut png_path = folder.clone();
    png_path.push(format!("Screenshot_{}.png", timestamp));
    let png_path_str = png_path.to_str().unwrap();

    // Save to file
    match image::save_buffer(png_path_str, &sdr_pixels, frame.width, frame.height, image::ColorType::Rgba8) {
        Ok(()) => println!("✅ Saved SDR screenshot to {}", png_path_str),
        Err(e) => eprintln!("❌ SDR save error: {:?}", e),
    }

    // Copy to clipboard
    match clipboard::copy_to_clipboard(&sdr_pixels, frame.width, frame.height) {
        Ok(()) => println!("✅ Copied to clipboard! Ctrl+V to paste"),
        Err(e) => eprintln!("❌ Clipboard error: {e}"),
    }

    // 3. If the screen is HDR, also save the original HDR data as a JXR file
    if frame.is_hdr {
        let mut jxr_path = folder.clone();
        jxr_path.push(format!("Screenshot_HDR_{}.jxr", timestamp));
        let jxr_path_str = jxr_path.to_str().unwrap();

        match save::save_as_jxr(jxr_path_str, frame.width, frame.height, &frame.pixels) {
            Ok(()) => println!("✅ Saved HDR screenshot to {}", jxr_path_str),
            Err(e) => eprintln!("❌ HDR save error: {:?}", e),
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

fn capture_settings(frame: &capture::CapturedFrame, defaults: tonemap::TonemapSettings) -> tonemap::TonemapSettings {
    if frame.is_hdr {
        // Use the system's SDR white level to normalize boosted SDR content
        // This ensures: boosted SDR (e.g., 2.0) → normalized to 1.0
        // Then when viewed on same HDR monitor, Windows boosts it back to match visual appearance
        tonemap::TonemapSettings {
            reference_white: frame.sdr_white_level.unwrap_or(1.0),
        }
    } else {
        defaults
    }
}

fn main() {
    println!("╔══════════════════════════════════════════╗");
    println!("║   HDR Screenshot Tool                    ║");
    println!("║   Press [Ctrl+Alt+A] to copy SDR to Clip ║");
    println!("║   Press [Ctrl+Alt+H] to save HDR to JXR  ║");
    println!("║   Press [PrtSrc] to directly save & copy ║");
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

        // Store main thread ID for the hook to send messages to
        MAIN_THREAD_ID.store(GetCurrentThreadId(), Ordering::SeqCst);

        // Install the low-level keyboard hook to capture PrtSrc
        let hook = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(low_level_keyboard_proc),
            HINSTANCE::default(),
            0,
        );

        match &hook {
            Ok(_) => println!("   PrtSrc keyboard hook registered successfully."),
            Err(e) => {
                eprintln!("⚠️ Warning: Failed to register PrtSrc keyboard hook ({:?}).", e);
                eprintln!("   Default Windows Snipping Tool behavior might not be overridden.");
            }
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
            } else if msg.message == WM_USER_TAKE_SCREENSHOT {
                take_direct_screenshot();
                println!("\n🎯 Waiting for hotkeys...\n");
            }
        }

        let _ = UnregisterHotKey(HWND::default(), HOTKEY_SDR_ID);
        let _ = UnregisterHotKey(HWND::default(), HOTKEY_HDR_ID);
        if let Ok(h) = hook {
            let _ = UnhookWindowsHookEx(h);
        }
    }
}
