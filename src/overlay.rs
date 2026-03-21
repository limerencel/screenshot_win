use std::cell::RefCell;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub struct SelectionRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

struct OverlayData {
    width: i32,
    height: i32,
    dc_original: HDC,
    dc_dimmed: HDC,
    bmp_original: HBITMAP,
    bmp_dimmed: HBITMAP,
    dragging: bool,
    start_x: i32,
    start_y: i32,
    cur_x: i32,
    cur_y: i32,
    confirmed: bool,
}

thread_local! {
    static DATA: RefCell<Option<OverlayData>> = RefCell::new(None);
}

/// Show fullscreen overlay with the tone-mapped screenshot.
/// User drags to select a region. Returns None if cancelled (ESC / right-click).
pub fn show_selection(srgb_rgba: &[u8], width: u32, height: u32) -> Option<SelectionRect> {
    let original_bgra = rgba_to_bgra(srgb_rgba);
    let dimmed_bgra = make_dimmed(&original_bgra);
    let w = width as i32;
    let h = height as i32;

    unsafe {
        let hmodule = GetModuleHandleW(None).unwrap_or_default();
        let hinstance = HINSTANCE::from(hmodule);
        let class_name = w!("HDROverlay");

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance.into(),
            hCursor: LoadCursorW(None, IDC_CROSS).unwrap_or_default(),
            hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
            lpszClassName: class_name,
            ..Default::default()
        };
        RegisterClassExW(&wc);

        // Create hidden first, set up GDI, then show
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class_name,
            w!(""),
            WS_POPUP,
            0,
            0,
            w,
            h,
            None,
            None,
            Some((&hinstance).into()),
            None,
        )
        .unwrap();

        // Set up GDI bitmaps
        let screen_dc = GetDC(hwnd);
        let dc_orig = CreateCompatibleDC(screen_dc);
        let dc_dim = CreateCompatibleDC(screen_dc);

        let bmp_orig = create_dib(screen_dc, w, h, &original_bgra);
        let bmp_dim = create_dib(screen_dc, w, h, &dimmed_bgra);

        SelectObject(dc_orig, bmp_orig);
        SelectObject(dc_dim, bmp_dim);
        ReleaseDC(hwnd, screen_dc);

        // Store state
        DATA.with(|cell| {
            *cell.borrow_mut() = Some(OverlayData {
                width: w,
                height: h,
                dc_original: dc_orig,
                dc_dimmed: dc_dim,
                bmp_original: bmp_orig,
                bmp_dimmed: bmp_dim,
                dragging: false,
                start_x: 0,
                start_y: 0,
                cur_x: 0,
                cur_y: 0,
                confirmed: false,
            });
        });

        // Now show
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);

        // Message loop
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Get result
        let result = DATA.with(|cell| {
            let data = cell.borrow();
            let d = data.as_ref().unwrap();
            if d.confirmed {
                let x1 = d.start_x.min(d.cur_x).max(0) as u32;
                let y1 = d.start_y.min(d.cur_y).max(0) as u32;
                let x2 = d.start_x.max(d.cur_x).min(w) as u32;
                let y2 = d.start_y.max(d.cur_y).min(h) as u32;
                if x2 > x1 + 2 && y2 > y1 + 2 {
                    Some(SelectionRect { x: x1, y: y1, w: x2 - x1, h: y2 - y1 })
                } else {
                    None
                }
            } else {
                None
            }
        });

        // Cleanup
        DATA.with(|cell| {
            if let Some(d) = cell.borrow().as_ref() {
                let _ = DeleteDC(d.dc_original);
                let _ = DeleteDC(d.dc_dimmed);
                let _ = DeleteObject(d.bmp_original);
                let _ = DeleteObject(d.bmp_dimmed);
            }
            *cell.borrow_mut() = None;
        });
        let _ = DestroyWindow(hwnd);
        let _ = UnregisterClassW(class_name, Some((&hinstance).into()));

        result
    }
}

unsafe fn create_dib(hdc: HDC, w: i32, h: i32, bgra: &[u8]) -> HBITMAP {
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0 as u32,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [RGBQUAD::default()],
    };
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let bmp = CreateDIBSection(hdc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
        .expect("CreateDIBSection failed");
    std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits as *mut u8, bgra.len());
    bmp
}

fn rgba_to_bgra(rgba: &[u8]) -> Vec<u8> {
    let mut bgra = Vec::with_capacity(rgba.len());
    for chunk in rgba.chunks_exact(4) {
        bgra.push(chunk[2]); // B
        bgra.push(chunk[1]); // G
        bgra.push(chunk[0]); // R
        bgra.push(chunk[3]); // A
    }
    bgra
}

fn make_dimmed(bgra: &[u8]) -> Vec<u8> {
    bgra.iter()
        .enumerate()
        .map(|(i, &v)| {
            if i % 4 == 3 { v } // keep alpha
            else { ((v as f32 * 0.55).max(18.0)).round() as u8 } // dim RGB without collapsing dark scenes to black
        })
        .collect()
}

fn loword(l: LPARAM) -> i32 {
    (l.0 & 0xFFFF) as i16 as i32
}
fn hiword(l: LPARAM) -> i32 {
    ((l.0 >> 16) & 0xFFFF) as i16 as i32
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            DATA.with(|cell| {
                if let Some(d) = cell.borrow().as_ref() {
                    let mem_dc = CreateCompatibleDC(hdc);
                    let mem_bmp = CreateCompatibleBitmap(hdc, d.width, d.height);
                    let old_bmp = SelectObject(mem_dc, mem_bmp);

                    // Draw dimmed background to off-screen buffer
                    let _ = BitBlt(mem_dc, 0, 0, d.width, d.height, d.dc_dimmed, 0, 0, SRCCOPY);

                    if d.dragging || d.confirmed {
                        let l = d.start_x.min(d.cur_x);
                        let t = d.start_y.min(d.cur_y);
                        let r = d.start_x.max(d.cur_x);
                        let b = d.start_y.max(d.cur_y);
                        let sw = r - l;
                        let sh = b - t;
                        if sw > 0 && sh > 0 {
                            // Bright original in selection area
                            let _ = BitBlt(mem_dc, l, t, sw, sh, d.dc_original, l, t, SRCCOPY);
                            // Border
                            let pen = CreatePen(PS_SOLID, 1, COLORREF(0x00D77800));
                            let old_pen = SelectObject(mem_dc, pen);
                            let old_brush = SelectObject(mem_dc, GetStockObject(NULL_BRUSH));
                            let _ = Rectangle(mem_dc, l, t, r, b);
                            SelectObject(mem_dc, old_pen);
                            SelectObject(mem_dc, old_brush);
                            let _ = DeleteObject(pen);
                        }
                    }

                    // Copy fully composed frame to screen
                    let _ = BitBlt(hdc, 0, 0, d.width, d.height, mem_dc, 0, 0, SRCCOPY);

                    // Cleanup
                    SelectObject(mem_dc, old_bmp);
                    let _ = DeleteObject(mem_bmp);
                    let _ = DeleteDC(mem_dc);
                }
            });
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            DATA.with(|cell| {
                if let Some(d) = cell.borrow_mut().as_mut() {
                    d.dragging = true;
                    d.start_x = loword(lparam);
                    d.start_y = hiword(lparam);
                    d.cur_x = d.start_x;
                    d.cur_y = d.start_y;
                }
            });
            SetCapture(hwnd);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            DATA.with(|cell| {
                if let Some(d) = cell.borrow_mut().as_mut() {
                    if d.dragging {
                        d.cur_x = loword(lparam);
                        d.cur_y = hiword(lparam);
                        let _ = InvalidateRect(hwnd, None, false);
                    }
                }
            });
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let should_quit = DATA.with(|cell| {
                if let Some(d) = cell.borrow_mut().as_mut() {
                    if d.dragging {
                        d.dragging = false;
                        d.cur_x = loword(lparam);
                        d.cur_y = hiword(lparam);
                        d.confirmed = true;
                        return true;
                    }
                }
                false
            });
            if should_quit {
                let _ = ReleaseCapture();
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        WM_RBUTTONDOWN => {
            let _ = ReleaseCapture();
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wparam.0 == VK_ESCAPE.0 as usize {
                let _ = ReleaseCapture();
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
