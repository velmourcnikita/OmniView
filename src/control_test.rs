//! Explicit native test: inject only while this process's own test window is foreground.
use std::time::{Duration, Instant};
use tao::{
    event_loop::{ControlFlow, EventLoopBuilder},
    platform::windows::WindowExtWindows,
    window::WindowBuilder,
};
use windows_sys::Win32::{
    Foundation::{POINT, RECT},
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};
pub fn run() -> Result<(), String> {
    let event_loop = EventLoopBuilder::<()>::with_user_event().build();
    let window = WindowBuilder::new()
        .with_title("OmniView · проверка мыши и клавиатуры")
        .with_inner_size(tao::dpi::LogicalSize::new(520.0, 190.0))
        .with_position(tao::dpi::LogicalPosition::new(80.0, 80.0))
        .with_focused(true)
        .build(&event_loop)
        .map_err(|e| e.to_string())?;
    let hwnd = window.hwnd() as windows_sys::Win32::Foundation::HWND;
    let edit = unsafe {
        let class: Vec<u16> = "EDIT\0".encode_utf16().collect();
        let empty = [0u16];
        let edit = CreateWindowExW(
            WS_EX_CLIENTEDGE,
            class.as_ptr(),
            empty.as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL as u32,
            24,
            55,
            450,
            38,
            hwnd,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
        );
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
        SetFocus(edit);
        edit
    };
    if edit.is_null() {
        return Err("Не удалось создать тестовое поле".into());
    }
    let gate = crate::control::InputGate::default();
    let start = Instant::now();
    let mut injected = false;
    let mut finished = false;
    let mut mouse_reached = false;
    let mut failure = None;
    let mut saved: POINT = unsafe { std::mem::zeroed() };
    unsafe {
        GetCursorPos(&mut saved);
    }
    let mut target: POINT = saved;
    event_loop.run(move|event,_,flow|{
        if finished { *flow=ControlFlow::Exit; return; }
        *flow=ControlFlow::WaitUntil(Instant::now()+Duration::from_millis(50));
        if injected {
            let mut observed:POINT=unsafe{std::mem::zeroed()};unsafe{GetCursorPos(&mut observed);}
            mouse_reached |= (observed.x-target.x).abs()<=2&&(observed.y-target.y).abs()<=2;
        }
        if matches!(event,tao::event::Event::WindowEvent{event:tao::event::WindowEvent::CloseRequested,..}){failure=Some("Проверка закрыта".to_string());}
        if !injected&&start.elapsed()>Duration::from_millis(350){
            injected=true;
            if unsafe{GetForegroundWindow()}!=hwnd{failure=Some("Тестовое окно не на переднем плане; ввод не выполнялся".into());}
            else{
                unsafe{SetFocus(edit);let mut r:RECT=std::mem::zeroed();GetWindowRect(edit,&mut r);target=POINT{x:r.left+20,y:r.top+15};}
                let sw=unsafe{GetSystemMetrics(SM_CXSCREEN)};let sh=unsafe{GetSystemMetrics(SM_CYSCREEN)};
                if sw<=1||sh<=1||target.x<0||target.y<0||target.x>=sw||target.y>=sh {failure=Some("Тестовое окно вне основного монитора".into());}
                else {
                    use crate::control::InputEvent as I;
                    gate.authorize("explicit-native-test".into());
                    let events=[I::Move{x:(target.x as i64*65535/(sw-1) as i64) as u16,y:(target.y as i64*65535/(sh-1) as i64) as u16},I::Left{down:true},I::Left{down:false},I::Text{text:"OmniView Я".into()},I::Key{vk:VK_BACK,ctrl:false,alt:false,shift:false},I::Text{text:"✓".into()}];
                    if let Err(e)=gate.deliver("explicit-native-test",&events){failure=Some(e);}
                }
            }
        }
        if failure.is_some()||start.elapsed()>Duration::from_millis(1100){
            finished=true;
            gate.revoke();let mut text=[0u16;80];let n=unsafe{GetWindowTextW(edit,text.as_mut_ptr(),text.len() as i32)};
            let text=String::from_utf16_lossy(&text[..n.max(0) as usize]);let mut cursor:POINT=unsafe{std::mem::zeroed()};unsafe{GetCursorPos(&mut cursor);}
            let still_at_target=(cursor.x-target.x).abs()<=2&&(cursor.y-target.y).abs()<=2;
            let mouse=mouse_reached||still_at_target;
            if injected&&still_at_target{unsafe{SetCursorPos(saved.x,saved.y);}}
            let ok=failure.is_none()&&text=="OmniView ✓"&&mouse;
            let _=std::fs::create_dir_all(crate::app::data_dir());
            let report=serde_json::json!({"ok":ok,"version":env!("CARGO_PKG_VERSION"),"arch":std::env::consts::ARCH,"own_window_only":true,"unicode_and_backspace":text=="OmniView ✓","absolute_mouse_and_click":mouse,"target":[target.x,target.y],"cursor":[cursor.x,cursor.y],"error":failure});
            let _=std::fs::write(crate::app::data_dir().join("control-smoke.json"),report.to_string());
            *flow=ControlFlow::Exit;
        }
    });
}
