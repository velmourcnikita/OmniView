#![windows_subsystem = "windows"]
mod app;
mod assessment;
mod capture;
mod classroom;
mod compatibility;
mod control;
mod control_test;
mod invitation;
mod lesson;
mod notifications;
mod updates;
mod video_quality;
use std::{borrow::Cow, sync::Arc, time::Duration};
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::WindowBuilder,
};
use wry::{
    http::Response, MemoryUsageLevel, WebContext, WebViewBuilder, WebViewBuilderExtWindows,
    WebViewExtWindows,
};
#[derive(Debug)]
enum UiEvent {
    Ipc(String),
    Tick,
}
fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|s| s == "--apply-update") {
        if let Some(path) = args.get(2) {
            if let Err(e) = updates::apply(std::path::Path::new(path)) {
                let _ = std::fs::write(app::data_dir().join("update-error.txt"), &e);
                unsafe {
                    use windows_sys::Win32::UI::WindowsAndMessaging::*;
                    let text: Vec<u16> = e.encode_utf16().chain(Some(0)).collect();
                    let title: Vec<u16> = "OmniView — обновление"
                        .encode_utf16()
                        .chain(Some(0))
                        .collect();
                    MessageBoxW(
                        std::ptr::null_mut(),
                        text.as_ptr(),
                        title.as_ptr(),
                        MB_ICONERROR,
                    );
                }
            }
        }
        return;
    }
    let _ = rustls::crypto::ring::default_provider().install_default();
    if std::env::args().any(|a| a == "--control-test") {
        if let Err(e) = control_test::run() {
            let _ = std::fs::create_dir_all(app::data_dir());
            let _ = std::fs::write(
                app::data_dir().join("control-smoke.json"),
                serde_json::json!({"ok":false,"error":e}).to_string(),
            );
        }
        return;
    }
    if std::env::args().any(|a| a == "--notification-test") {
        if let Err(e) = notifications::smoke_test() {
            let _ = std::fs::create_dir_all(app::data_dir());
            let _ = std::fs::write(
                app::data_dir().join("notification-smoke.json"),
                serde_json::json!({"ok":false,"error":e}).to_string(),
            );
        }
        return;
    }
    let commands: Vec<_> = std::env::args().collect();
    if commands
        .get(1)
        .is_some_and(|a| a == "--compat-server" || a == "--compat-client")
    {
        let result = commands
            .get(2)
            .ok_or("No test folder".to_string())
            .and_then(|p| compatibility::run(&commands[1], std::path::Path::new(p)));
        std::process::exit(if result.is_ok() { 0 } else { 1 });
    }
    if std::env::args().any(|a| a == "--update-check") {
        let updates = updates::Updates::new();
        let result = updates.check(std::env::args().any(|a| a == "--preview"));
        let _ = std::fs::create_dir_all(app::data_dir());
        let _ = std::fs::write(
            app::data_dir().join("update-check.json"),
            updates::summary(&updates).to_string(),
        );
        std::process::exit(if result.is_ok() { 0 } else { 1 });
    }
    if std::env::args().any(|a| a == "--self-test") {
        let result = app::self_test();
        let _ = std::fs::create_dir_all(app::data_dir());
        let _ = std::fs::write(app::data_dir().join("self-test.json"), result.to_string());
        std::process::exit(if result["ok"] == true { 0 } else { 1 });
    }
    if let Err(error) = run() {
        let _ = std::fs::create_dir_all(app::data_dir());
        let _ = std::fs::write(app::data_dir().join("startup-error.txt"), &error);
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR};
            let t: Vec<u16> = "OmniView\0".encode_utf16().collect();
            let m:Vec<u16>=format!("Не удалось запустить OmniView.\nНужны Windows 10/11 и Microsoft Edge WebView2 Runtime.\n\n{error}\0").encode_utf16().collect();
            MessageBoxW(std::ptr::null_mut(), m.as_ptr(), t.as_ptr(), MB_ICONERROR);
        }
    }
}
fn run() -> Result<(), String> {
    let smoke = std::env::args().any(|a| a == "--ui-smoke");
    let student = std::env::args().any(|a| a == "--student")
        || std::env::current_exe()
            .ok()
            .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_lowercase()))
            .is_some_and(|s| s.contains("student"));
    let core = Arc::new(app::Core::new(student));
    if !smoke && core.snapshot()["prefs"]["auto_updates"] == true {
        let c = core.clone();
        std::thread::spawn(move || {
            let _ = c.handle(serde_json::json!({"action":"check_updates"}));
        });
    }
    let event_loop = EventLoopBuilder::<UiEvent>::with_user_event().build();
    let window = WindowBuilder::new()
        .with_visible(!smoke)
        .with_title(if student {
            "OmniView · ученик"
        } else {
            "OmniView · класс"
        })
        .with_inner_size(tao::dpi::LogicalSize::new(1240.0, 810.0))
        .with_min_inner_size(tao::dpi::LogicalSize::new(820.0, 600.0))
        .build(&event_loop)
        .map_err(|e| e.to_string())?;
    let mut context = WebContext::new(Some(app::data_dir().join(if student {
        "webview-student"
    } else {
        "webview-teacher"
    })));
    let proxy = event_loop.create_proxy();
    let asset_core = core.clone();
    let webview=WebViewBuilder::new_with_web_context(&mut context)
  .with_custom_protocol("omniview".into(),move|_,request|{
   if request.uri().path().starts_with("/preview/"){
    return match asset_core.image(request.uri().path()){
     Some(bytes)=>Response::builder().header("Content-Type","image/jpeg").header("Cache-Control","no-store").body(Cow::Owned(bytes)).unwrap(),
     None=>Response::builder().status(404).body(Cow::Borrowed(b"No shared screen" as &[u8])).unwrap()
    };
   }
   let (mime,content):(&str,&'static[u8])=match request.uri().path(){
    "/"|"/index.html"=>("text/html; charset=utf-8",include_bytes!("../web/index.html")),
    "/app.js"=>("text/javascript; charset=utf-8",include_bytes!("../web/app.js")),
    "/control.js"=>("text/javascript; charset=utf-8",include_bytes!("../web/control.js")),
    "/lesson.js"=>("text/javascript; charset=utf-8",include_bytes!("../web/lesson.js")),
    "/styles.css"=>("text/css; charset=utf-8",include_bytes!("../web/styles.css")),
    "/AppRegular.ttf"=>("font/ttf",include_bytes!("../assets/fonts/AppRegular.ttf")),
    "/AppBold.ttf"=>("font/ttf",include_bytes!("../assets/fonts/AppBold.ttf")),
    _=>return Response::builder().status(404).body(Cow::Borrowed(b"Not found" as &[u8])).unwrap()
   };
   Response::builder().header("Content-Type",mime).header("Content-Security-Policy","default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; font-src 'self'; connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'")
    .header("X-Content-Type-Options","nosniff").body(Cow::Borrowed(content)).unwrap()
  })
  .with_ipc_handler(move|request|{let u=request.uri().to_string();if request.body().len()<=32_768 && (u.starts_with("http://omniview.localhost/") || u.starts_with("omniview://localhost/")){let _=proxy.send_event(UiEvent::Ipc(request.into_body()));}})
  .with_navigation_handler(|u|u=="http://omniview.localhost/" || u=="omniview://localhost/" || u=="omniview://localhost")
  .with_new_window_req_handler(|_,_|wry::NewWindowResponse::Deny)
  .with_devtools(false).with_autoplay(false).with_general_autofill_enabled(false).with_browser_extensions_enabled(false)
  .with_background_color((15,21,18,255)).with_url("omniview://localhost/").build(&window).map_err(|e|e.to_string())?;
    let timer = event_loop.create_proxy();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(250));
        if timer.send_event(UiEvent::Tick).is_err() {
            break;
        }
    });
    let mut ready = false;
    let mut last = 0;
    let mut smoke_sent = false;
    let mut notifications = notifications::Notifications::new(&event_loop)?;
    event_loop.run(move|event,_,flow|{
  *flow=ControlFlow::Wait;
  match event{
   Event::UserEvent(UiEvent::Ipc(raw))=>if let Ok(value)=serde_json::from_str::<serde_json::Value>(&raw){
    if smoke && value["action"]=="ui_smoke"{
     let _=std::fs::write(app::data_dir().join("ui-smoke.json"),value.to_string());core.stop();*flow=ControlFlow::Exit;
    }else if value["action"]=="install_update"{match core.handle(value){Ok(())=>{core.stop();*flow=ControlFlow::Exit;},Err(e)=>core.feedback(&e)}}else if value["action"]=="ready"{ready=true;last=0;}else if value["action"]=="control_input"{if let Err(e)=core.handle(value){core.feedback(&e);}}else{let core=core.clone();std::thread::spawn(move||{if let Err(e)=core.handle(value){core.feedback(&e);}});}
   },
   Event::WindowEvent{window_id,event:WindowEvent::CloseRequested,..}=>{if window_id==notifications.window.id(){if notifications.assisting{let _=core.handle(serde_json::json!({"action":"control_consent","on":false}));}notifications.dismiss();}else{core.stop();*flow=ControlFlow::Exit;}},
   Event::WindowEvent{window_id,event:WindowEvent::MouseInput{state:tao::event::ElementState::Pressed,..},..} if window_id==notifications.window.id()=>{if notifications.assisting{let _=core.handle(serde_json::json!({"action":"control_consent","on":false}));}notifications.dismiss();},
   Event::RedrawRequested(id) if id==notifications.window.id()=>notifications.paint(),
   Event::WindowEvent{event:WindowEvent::Focused(active),..}=>{let _=webview.set_memory_usage_level(if active{MemoryUsageLevel::Normal}else{MemoryUsageLevel::Low});},
   _=>{}
  }
  notifications.tick();
  let revision=core.revision();if ready && revision!=last{
   let snapshot=core.snapshot();window.set_title(if snapshot["role"]=="student"{"OmniView · ученик"}else{"OmniView · класс"});
   if !smoke {notifications.update(&snapshot);}
   if webview.evaluate_script(&format!("window.receiveState({snapshot});")).is_ok(){last=revision;}
   if smoke && !smoke_sent{smoke_sent=true;let _=webview.evaluate_script("document.fonts.ready.then(()=>window.ipc.postMessage(JSON.stringify({action:'ui_smoke',font:document.fonts.check('14px \"Google Sans\"'),bridge:document.querySelector('#boot').hidden,search:document.querySelector('#search') instanceof HTMLInputElement,title:Array.from(document.querySelectorAll('h1')).find(e=>e.getClientRects().length)?.textContent,buttons:document.querySelectorAll('button').length,role:document.querySelector('#profileRole').textContent}))); ");}
  }
 });
}
