//! Application-owned notification window. Drawn with GDI; no extra WebView process.
use serde_json::Value;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
use tao::{
    event_loop::EventLoopWindowTarget,
    platform::windows::{WindowBuilderExtWindows, WindowExtWindows},
    window::{Window, WindowBuilder},
};
pub struct Notifications {
    pub window: Window,
    title: String,
    text: String,
    until: Option<Instant>,
    seen: VecDeque<String>,
    initialized: bool,
}
impl Notifications {
    pub fn save_preview(&self) -> Result<(), String> {
        use windows_sys::Win32::Graphics::Gdi::*;
        unsafe {
            let hwnd = self.window.hwnd() as _;
            let source = GetDC(hwnd);
            if source.is_null() {
                return Err("Нет DC уведомления".into());
            }
            let size = self.window.inner_size();
            let w = size.width as i32;
            let h = size.height as i32;
            let dc = CreateCompatibleDC(source);
            let bitmap = CreateCompatibleBitmap(source, w, h);
            if dc.is_null() || bitmap.is_null() {
                if !bitmap.is_null() {
                    DeleteObject(bitmap as _);
                }
                if !dc.is_null() {
                    DeleteDC(dc);
                }
                ReleaseDC(hwnd, source);
                return Err("Нет ресурсов уведомления".into());
            }
            let old = SelectObject(dc, bitmap as _);
            BitBlt(dc, 0, 0, w, h, source, 0, 0, SRCCOPY);
            SelectObject(dc, old);
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = w;
            info.bmiHeader.biHeight = -h;
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            let mut pixels = vec![0u8; (w * h * 4) as usize];
            let rows = GetDIBits(
                dc,
                bitmap,
                0,
                h as u32,
                pixels.as_mut_ptr() as _,
                &mut info,
                DIB_RGB_COLORS,
            );
            DeleteObject(bitmap as _);
            DeleteDC(dc);
            ReleaseDC(hwnd, source);
            if rows != h {
                return Err("Не удалось прочитать уведомление".into());
            }
            let rgb = pixels
                .chunks_exact(4)
                .flat_map(|p| [p[2], p[1], p[0]])
                .collect::<Vec<_>>();
            let mut jpeg = Vec::new();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
                .encode(&rgb, w as u32, h as u32, image::ExtendedColorType::Rgb8)
                .map_err(|e| e.to_string())?;
            std::fs::create_dir_all(crate::app::data_dir()).map_err(|e| e.to_string())?;
            std::fs::write(
                crate::app::data_dir().join("notification-preview.jpg"),
                jpeg,
            )
            .map_err(|e| e.to_string())
        }
    }
    pub fn new<T>(target: &EventLoopWindowTarget<T>) -> Result<Self, String> {
        let window = WindowBuilder::new()
            .with_title("OmniView · уведомление")
            .with_visible(false)
            .with_focused(false)
            .with_decorations(false)
            .with_resizable(false)
            .with_skip_taskbar(true)
            .with_always_on_top(true)
            .with_inner_size(tao::dpi::LogicalSize::new(380.0, 164.0))
            .build(target)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            window,
            title: String::new(),
            text: String::new(),
            until: None,
            seen: VecDeque::new(),
            initialized: false,
        })
    }
    fn remember(&mut self, key: String) -> bool {
        if self.seen.contains(&key) {
            return false;
        }
        if self.seen.len() >= 256 {
            self.seen.pop_front();
        }
        self.seen.push_back(key);
        true
    }
    pub fn update(&mut self, s: &Value) {
        let student = s["role"] == "student";
        let messages = if student {
            s["student"]["messages"].as_array()
        } else {
            s["teacher"]["log"].as_array()
        };
        let mut next = None;
        if let Some(messages) = messages {
            for m in messages {
                let from = m["from"].as_str().unwrap_or("");
                if (student && from != "Вы") || (!student && from != "Учитель") {
                    if self.remember(format!("message:{m}")) {
                        next = Some((
                            from.to_string(),
                            m["text"].as_str().unwrap_or("").to_string(),
                        ));
                    }
                }
            }
        }
        if student {
            if let Some(id) = s["student"]["question"]["id"].as_str() {
                if self.remember(format!("question:{id}")) {
                    next = Some((
                        "Вопрос преподавателя".into(),
                        s["student"]["question"]["prompt"]
                            .as_str()
                            .unwrap_or("")
                            .into(),
                    ));
                }
            }
        } else {
            if let Some(peers) = s["teacher"]["peers"].as_array() {
                for p in peers {
                    if p["online"] == true
                        && p["help"] == true
                        && self.remember(format!("help:{}:{}", p["id"], p["name"]))
                    {
                        next = Some((
                            "Запрос помощи".into(),
                            p["name"].as_str().unwrap_or("Ученик").into(),
                        ));
                    } else if p["help"] != true {
                        let key = format!("help:{}:{}", p["id"], p["name"]);
                        self.seen.retain(|v| v != &key);
                    }
                }
            }
            if let Some(events) = s["teacher"]["assessment"]["events"].as_array() {
                for e in events {
                    if self.remember(format!("detection:{e}")) {
                        next = Some((
                            "Самостоятельная работа".into(),
                            format!(
                                "{} · обнаружен {}",
                                e["name"].as_str().unwrap_or("Ученик"),
                                e["executable"].as_str().unwrap_or("процесс")
                            ),
                        ));
                    }
                }
            }
        }
        if self.initialized {
            if let Some((title, text)) = next {
                self.show(&title, &text);
            }
        }
        self.initialized = true;
    }
    pub fn show(&mut self, title: &str, text: &str) {
        self.title = title.chars().take(70).collect();
        self.text = text.chars().take(220).collect();
        self.until = Some(Instant::now() + Duration::from_secs(9));
        if let Some(monitor) = self.window.current_monitor() {
            let p = monitor.position();
            let size = monitor.size();
            let own = self.window.outer_size();
            let margin = (24.0 * monitor.scale_factor()) as i32;
            self.window
                .set_outer_position(tao::dpi::PhysicalPosition::new(
                    p.x + size.width as i32 - own.width as i32 - margin,
                    p.y + size.height as i32 - own.height as i32 - margin - 60,
                ));
        }
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::ShowWindow(
                self.window.hwnd() as _,
                windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNOACTIVATE,
            );
        }
        unsafe {
            use windows_sys::Win32::Graphics::Gdi::*;
            let size = self.window.inner_size();
            let radius = (20.0 * self.window.scale_factor()) as i32;
            let region = CreateRoundRectRgn(
                0,
                0,
                size.width as i32 + 1,
                size.height as i32 + 1,
                radius * 2,
                radius * 2,
            );
            if SetWindowRgn(self.window.hwnd() as _, region, 1) == 0 {
                DeleteObject(region as _);
            }
        }
        self.window.request_redraw();
    }
    pub fn tick(&mut self) {
        if self.until.is_some_and(|until| Instant::now() >= until) {
            self.dismiss();
        }
    }
    pub fn dismiss(&mut self) {
        self.window.set_visible(false);
        self.until = None;
    }
    pub fn paint(&self) {
        use windows_sys::Win32::{
            Foundation::{RECT, SIZE},
            Graphics::Gdi::*,
        };
        unsafe {
            let hwnd = self.window.hwnd() as _;
            let target = GetDC(hwnd);
            if target.is_null() {
                return;
            }
            let size = self.window.inner_size();
            let scale = self.window.scale_factor();
            let px = |v: f64| (v * scale).round() as i32;
            let dc = CreateCompatibleDC(target);
            let bitmap = CreateCompatibleBitmap(target, size.width as i32, size.height as i32);
            if dc.is_null() || bitmap.is_null() {
                if !bitmap.is_null() {
                    DeleteObject(bitmap as _);
                }
                if !dc.is_null() {
                    DeleteDC(dc);
                }
                ReleaseDC(hwnd, target);
                return;
            }
            let old_bitmap = SelectObject(dc, bitmap as _);
            let fill = CreateSolidBrush(0x00262b25);
            let border = CreatePen(PS_SOLID, px(1.0).max(1), 0x00434d40);
            let old_brush = SelectObject(dc, fill as _);
            let old_pen = SelectObject(dc, border as _);
            RoundRect(
                dc,
                0,
                0,
                size.width as i32,
                size.height as i32,
                px(40.0),
                px(40.0),
            );
            let accent = CreateSolidBrush(0x00bdd4bb);
            SelectObject(dc, accent as _);
            let accent_pen = CreatePen(PS_SOLID, 1, 0x00bdd4bb);
            SelectObject(dc, accent_pen as _);
            RoundRect(
                dc,
                px(18.0),
                px(16.0),
                px(44.0),
                px(42.0),
                px(15.0),
                px(15.0),
            );
            let icon_pen = CreatePen(PS_SOLID, px(1.5).max(1), 0x00212c20);
            SelectObject(dc, icon_pen as _);
            Ellipse(dc, px(22.0), px(23.0), px(40.0), px(35.0));
            let pupil = CreateSolidBrush(0x00212c20);
            SelectObject(dc, pupil as _);
            Ellipse(dc, px(28.0), px(26.0), px(34.0), px(32.0));
            SelectObject(dc, old_brush);
            SelectObject(dc, old_pen);
            DeleteObject(fill as _);
            DeleteObject(border as _);
            DeleteObject(accent as _);
            DeleteObject(accent_pen as _);
            DeleteObject(icon_pen as _);
            DeleteObject(pupil as _);
            SetBkMode(dc, TRANSPARENT as i32);
            let close_pen = CreatePen(PS_SOLID, px(1.3).max(1), 0x00a0ab9f);
            let old = SelectObject(dc, close_pen as _);
            MoveToEx(dc, px(349.0), px(24.0), std::ptr::null_mut());
            LineTo(dc, px(357.0), px(32.0));
            MoveToEx(dc, px(357.0), px(24.0), std::ptr::null_mut());
            LineTo(dc, px(349.0), px(32.0));
            SelectObject(dc, old);
            DeleteObject(close_pen as _);
            let face: Vec<u16> = "Google Sans\0".encode_utf16().collect();
            let draw = |text: &str,
                        x: f64,
                        y: f64,
                        width: f64,
                        height: f64,
                        font_size: f64,
                        weight: i32,
                        color: u32| {
                let font = CreateFontW(
                    -px(font_size),
                    0,
                    0,
                    0,
                    weight,
                    0,
                    0,
                    0,
                    1,
                    0,
                    0,
                    5,
                    0,
                    face.as_ptr(),
                );
                let old = SelectObject(dc, font as _);
                SetTextColor(dc, color);
                let mut r = RECT {
                    left: px(x),
                    top: px(y),
                    right: px(x + width),
                    bottom: px(y + height),
                };
                let chars: Vec<u16> = text.encode_utf16().collect();
                DrawTextW(
                    dc,
                    chars.as_ptr(),
                    chars.len() as i32,
                    &mut r,
                    DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS,
                );
                SelectObject(dc, old);
                DeleteObject(font as _);
            };
            draw("OmniView", 54.0, 20.0, 200.0, 22.0, 13.0, 500, 0x00bdc8bc);
            draw(&self.title, 20.0, 57.0, 336.0, 24.0, 16.0, 600, 0x00eef4eb);
            let measure_font = CreateFontW(
                -px(14.0),
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                1,
                0,
                0,
                5,
                0,
                face.as_ptr(),
            );
            let old_font = SelectObject(dc, measure_font as _);
            let mut lines = Vec::new();
            let mut line = String::new();
            for word in self.text.split_whitespace() {
                let candidate = if line.is_empty() {
                    word.to_string()
                } else {
                    format!("{line} {word}")
                };
                let chars: Vec<u16> = candidate.encode_utf16().collect();
                let mut measured: SIZE = std::mem::zeroed();
                GetTextExtentPoint32W(dc, chars.as_ptr(), chars.len() as i32, &mut measured);
                if measured.cx > px(336.0) && !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                    line = word.to_string();
                } else {
                    line = candidate;
                }
            }
            if !line.is_empty() {
                lines.push(line);
            }
            SelectObject(dc, old_font);
            DeleteObject(measure_font as _);
            for (i, line) in lines.iter().take(3).enumerate() {
                let text = if i == 2 && lines.len() > 3 {
                    format!("{line}…")
                } else {
                    line.clone()
                };
                draw(
                    &text,
                    20.0,
                    86.0 + i as f64 * 21.0,
                    336.0,
                    21.0,
                    14.0,
                    400,
                    0x00bbc5b9,
                );
            }
            BitBlt(
                target,
                0,
                0,
                size.width as i32,
                size.height as i32,
                dc,
                0,
                0,
                SRCCOPY,
            );
            SelectObject(dc, old_bitmap);
            DeleteObject(bitmap as _);
            DeleteDC(dc);
            ReleaseDC(hwnd, target);
        }
    }
}

pub fn smoke_test() -> Result<(), String> {
    let event_loop = tao::event_loop::EventLoopBuilder::<()>::with_user_event().build();
    let mut notice = Notifications::new(&event_loop)?;
    notice.show(
        "Учитель",
        "Проверь второй пункт задания. Это уведомление OmniView.",
    );
    let started = Instant::now();
    event_loop.run(move |event, _, flow| {
        *flow =
            tao::event_loop::ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(100));
        if matches!(event, tao::event::Event::RedrawRequested(_)) {
            notice.paint();
        }
        if started.elapsed() > Duration::from_secs(1) {
            let result = notice.save_preview();
            let _ = std::fs::write(
                crate::app::data_dir().join("notification-smoke.json"),
                serde_json::json!({"ok":result.is_ok(),"error":result.err(),"native_window":true})
                    .to_string(),
            );
            notice.dismiss();
            *flow = tao::event_loop::ControlFlow::Exit;
        }
    });
}
