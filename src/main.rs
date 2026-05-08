#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use muda::{Menu, MenuEvent, MenuItem};
#[cfg(target_os = "macos")]
use muda::{PredefinedMenuItem, Submenu};
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::WindowBuilder,
};
#[cfg(target_os = "macos")]
use tao::event::StartCause;
use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use wry::WebViewBuilder;

#[cfg(target_os = "linux")]
use tao::platform::unix::WindowExtUnix;
#[cfg(target_os = "linux")]
use wry::WebViewBuilderExtUnix;

#[cfg(target_os = "macos")]
use tao::{dpi::LogicalPosition, platform::macos::WindowBuilderExtMacOS};

#[derive(Debug, Clone, Copy)]
enum UserEvent {
    DragWindow,
    PageLoaded,
    SetBadge(BadgeState),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BadgeState {
    None,
    Dot,
    Count(u32),
}

const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.6 Safari/605.1.15";

// Paints html/body dark before Discord's own CSS loads, killing the white flash
// during navigation. Runs at document_start on every page.
const DARK_BACKGROUND_SCRIPT: &str = r#"
(function () {
  const css = 'html,body{background-color:#000 !important;color-scheme:dark}';
  const inject = () => {
    const style = document.createElement('style');
    style.textContent = css;
    (document.head || document.documentElement).appendChild(style);
  };
  if (document.documentElement) inject();
  else document.addEventListener('readystatechange', inject, { once: true });
})();
"#;

// Blocks Discord analytics (/science, /track) and Sentry crash reports.
// Stays well within what consumer adblockers do on the web client.
const TRACKING_BLOCKER_SCRIPT: &str = r#"
(function () {
  const BLOCK_PATTERNS = [
    /\/api\/v\d+\/science\b/,
    /\/api\/v\d+\/track\b/,
    /sentry\.io/,
    /sentry\.discord\.com/,
    /crash\.discord\.com/,
  ];

  const isBlocked = (url) => {
    try { return BLOCK_PATTERNS.some((re) => re.test(String(url))); }
    catch (_) { return false; }
  };

  const origFetch = window.fetch;
  window.fetch = function (input, init) {
    const url = typeof input === 'string' ? input : (input && input.url) || '';
    if (isBlocked(url)) {
      return Promise.resolve(new Response(null, { status: 204, statusText: 'No Content' }));
    }
    return origFetch.call(this, input, init);
  };

  const origOpen = XMLHttpRequest.prototype.open;
  XMLHttpRequest.prototype.open = function (method, url, ...rest) {
    this.__dt_blocked = isBlocked(url);
    return origOpen.call(this, method, url, ...rest);
  };

  const origSend = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.send = function (body) {
    if (this.__dt_blocked) {
      Object.defineProperty(this, 'readyState', { value: 4, configurable: true });
      Object.defineProperty(this, 'status', { value: 204, configurable: true });
      Object.defineProperty(this, 'responseText', { value: '', configurable: true });
      setTimeout(() => {
        this.dispatchEvent(new Event('readystatechange'));
        this.dispatchEvent(new Event('load'));
        this.dispatchEvent(new Event('loadend'));
      }, 0);
      return;
    }
    return origSend.call(this, body);
  };

  if (navigator.sendBeacon) {
    const origBeacon = navigator.sendBeacon.bind(navigator);
    navigator.sendBeacon = function (url, data) {
      if (isBlocked(url)) return true;
      return origBeacon(url, data);
    };
  }
})();
"#;

// Watches document.title for Discord's unread/mention markers and forwards the
// state to native code so the tray icon and dock tile can display a badge.
//   "(N) ..."  -> N unread, "badge:N"
//   "• ..."    -> mentions without a count, "badge:dot"
//   otherwise  -> "badge:0"
const BADGE_OBSERVER_SCRIPT: &str = r#"
(function () {
  let last = null;
  const send = () => {
    const t = document.title || '';
    const m = t.match(/^\((\d+)\)/);
    const next = m ? ('badge:' + m[1]) : (/^•/.test(t) ? 'badge:dot' : 'badge:0');
    if (next === last) return;
    last = next;
    if (window.ipc && window.ipc.postMessage) window.ipc.postMessage(next);
  };
  // Title element appears mid-parse on Discord; wait for it before observing.
  const attach = () => {
    const t = document.querySelector('title');
    if (!t) return false;
    new MutationObserver(send).observe(t, { childList: true, characterData: true, subtree: true });
    send();
    return true;
  };
  if (!attach()) {
    const waiter = new MutationObserver(() => { if (attach()) waiter.disconnect(); });
    waiter.observe(document.documentElement, { childList: true, subtree: true });
  }
})();
"#;

// Vencord bundle — fetched + patched in build.rs.
const VENCORD_BROWSER_JS: &str = include_str!(concat!(env!("OUT_DIR"), "/vencord/browser.js"));
const VENCORD_BROWSER_CSS: &str = include_str!(concat!(env!("OUT_DIR"), "/vencord/browser.css"));

// Skips the `with_html` bootstrap doc (empty hostname, discarded by meta-refresh)
// and any non-Discord iframe — parsing/running Vencord there is pure waste.
const HOST_GATE: &str = "/(?:^|\\.)discord\\.com$/i.test(location.hostname)";

fn build_vencord_css_loader() -> String {
    let mut out = String::with_capacity(VENCORD_BROWSER_CSS.len() + 256);
    out.push_str("if(");
    out.push_str(HOST_GATE);
    out.push_str("){try{var s=document.createElement('style');s.id='vencord-css';s.textContent=");
    push_js_string_literal(&mut out, VENCORD_BROWSER_CSS);
    out.push_str(";(document.head||document.documentElement).appendChild(s);}catch(e){console.error('[discord-tauri] Vencord CSS inject failed',e);}}");
    out
}

fn build_vencord_js_loader() -> String {
    // Vencord's bundle is `"use strict";var Vencord=(()=>{...})();` — a single
    // expression statement assigning to a `var`, which is safe to wrap in an
    // `if` block (var hoists out, the IIFE only runs when the gate passes).
    let mut out = String::with_capacity(VENCORD_BROWSER_JS.len() + 128);
    out.push_str("if(");
    out.push_str(HOST_GATE);
    out.push_str("){\n");
    out.push_str(VENCORD_BROWSER_JS);
    out.push_str("\n}");
    out
}

fn push_js_string_literal(out: &mut String, s: &str) {
    use std::fmt::Write;
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // U+2028/2029 are valid in CSS but break JS string literals.
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(target_os = "macos")]
const DRAG_REGION_SCRIPT: &str = r#"
(function () {
  const TOP_HEIGHT = 32;
  const TRAFFIC_LIGHT_WIDTH = 80;

  const style = document.createElement('style');
  style.textContent = `
    html.discord-tauri-drag-hover, html.discord-tauri-drag-hover * { cursor: grab !important; }
    html.discord-tauri-dragging, html.discord-tauri-dragging * { cursor: grabbing !important; }
  `;
  (document.head || document.documentElement).appendChild(style);

  const root = document.documentElement;
  const inZone = (e) => e.clientY < TOP_HEIGHT && e.clientX > TRAFFIC_LIGHT_WIDTH;

  document.addEventListener('mousemove', (e) => {
    root.classList.toggle('discord-tauri-drag-hover', inZone(e));
  }, true);

  document.addEventListener('mousedown', (e) => {
    if (e.button === 0 && inZone(e) && window.ipc && window.ipc.postMessage) {
      root.classList.add('discord-tauri-dragging');
      window.ipc.postMessage('drag');
    }
  }, true);

  const clearDragging = () => root.classList.remove('discord-tauri-dragging');
  document.addEventListener('mouseup', clearDragging, true);
  window.addEventListener('blur', clearDragging);
})();
"#;

fn main() -> wry::Result<()> {
    #[cfg(target_os = "linux")]
    unsafe {
        std::env::set_var("GDK_BACKEND", "wayland")
    };

    // Warm the OS DNS resolver before the WebView is even created. WKWebView
    // shares the system resolver, so this shaves the first DNS round-trip off
    // the cold-start path.
    std::thread::spawn(|| {
        use std::net::ToSocketAddrs;
        for host in [
            "discord.com:443",
            "gateway.discord.gg:443",
            "cdn.discordapp.com:443",
            "media.discordapp.net:443",
        ] {
            let _ = host.to_socket_addrs();
        }
    });

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();

    #[cfg(target_os = "macos")]
    init_macos_app_menu();

    let window_builder = WindowBuilder::new()
        .with_title("Discord")
        .with_visible(false)
        .with_window_icon(load_window_icon())
        .with_inner_size(tao::dpi::LogicalSize::new(1280.0, 800.0));

    #[cfg(target_os = "macos")]
    let window_builder = window_builder
        .with_titlebar_transparent(true)
        .with_fullsize_content_view(true)
        .with_title_hidden(true)
        .with_traffic_light_inset(LogicalPosition::new(18.0, 18.0));

    let window = window_builder
        .build(&event_loop)
        .expect("Failed to build window");

    #[cfg(target_os = "macos")]
    set_macos_window_background(&window);

    // Tiny dark bootstrap doc — paints black instantly via with_html (no
    // network), then `<meta refresh>` triggers the real navigation. The
    // browser handles refresh natively, no JS engine spin-up needed.
    let html = "<!doctype html><meta http-equiv=refresh content=\"0;url=https://discord.com/app\"><style>html,body{background:#000;margin:0;height:100%}</style>";

    let proxy = event_loop.create_proxy();
    let ipc_handler = move |req: wry::http::Request<String>| {
        let body = req.body();
        if body == "drag" {
            let _ = proxy.send_event(UserEvent::DragWindow);
        } else if let Some(rest) = body.strip_prefix("badge:") {
            let state = match rest {
                "0" => BadgeState::None,
                "dot" => BadgeState::Dot,
                n => n.parse::<u32>().map(BadgeState::Count).unwrap_or(BadgeState::None),
            };
            let _ = proxy.send_event(UserEvent::SetBadge(state));
        }
    };

    let load_proxy = event_loop.create_proxy();
    let load_fired = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let load_fired_cb = load_fired.clone();
    let on_page_load = move |event: wry::PageLoadEvent, _url: String| {
        if matches!(event, wry::PageLoadEvent::Started)
            && !load_fired_cb.swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            let _ = load_proxy.send_event(UserEvent::PageLoaded);
        }
    };

    let vencord_disabled = std::env::var("DT_NO_VENCORD").is_ok();
    let force_devtools = std::env::var("DT_DEVTOOLS").is_ok();

    let mut webview_builder = WebViewBuilder::new()
        .with_html(html)
        .with_user_agent(USER_AGENT)
        .with_autoplay(true)
        .with_devtools(cfg!(debug_assertions) || force_devtools)
        .with_background_color((0, 0, 0, 255))
        .with_initialization_script(DARK_BACKGROUND_SCRIPT)
        .with_initialization_script(TRACKING_BLOCKER_SCRIPT)
        .with_initialization_script(BADGE_OBSERVER_SCRIPT)
        .with_on_page_load_handler(on_page_load)
        .with_ipc_handler(ipc_handler);

    let vencord_css_loader;
    let vencord_js_loader;
    if !vencord_disabled {
        vencord_css_loader = build_vencord_css_loader();
        vencord_js_loader = build_vencord_js_loader();
        webview_builder = webview_builder
            .with_initialization_script(&vencord_css_loader)
            .with_initialization_script(&vencord_js_loader);
    }

    #[cfg(target_os = "macos")]
    let webview_builder = webview_builder.with_initialization_script(DRAG_REGION_SCRIPT);

    #[cfg(target_os = "linux")]
    let _webview = {
        let vbox = window.default_vbox().expect("Failed to get vbox");
        webview_builder
            .build_gtk(vbox)
            .expect("Failed to build WebView")
    };

    #[cfg(not(target_os = "linux"))]
    let _webview = webview_builder
        .build(&window)
        .expect("Failed to build WebView");

    let tray_menu = Menu::new();
    let quit_item = MenuItem::new("Quit", true, None);

    #[cfg(debug_assertions)]
    let (test_dot_id, test_count_id, test_big_id, test_clear_id, test_resume_id) = {
        use muda::PredefinedMenuItem;
        let dot = MenuItem::new("Test: badge dot", true, None);
        let count = MenuItem::new("Test: badge 5", true, None);
        let big = MenuItem::new("Test: badge 99+", true, None);
        let clear = MenuItem::new("Test: clear badge", true, None);
        let resume = MenuItem::new("Test: resume auto", true, None);
        tray_menu
            .append_items(&[
                &dot,
                &count,
                &big,
                &clear,
                &resume,
                &PredefinedMenuItem::separator(),
            ])
            .unwrap();
        (
            dot.id().clone(),
            count.id().clone(),
            big.id().clone(),
            clear.id().clone(),
            resume.id().clone(),
        )
    };

    tray_menu.append(&quit_item).unwrap();
    let quit_id = quit_item.id().clone();

    let tray = TrayIconBuilder::new()
        .with_icon(load_tray_icon_plain().expect("Failed to load icon"))
        .with_icon_as_template(cfg!(target_os = "macos"))
        .with_tooltip("Discord")
        .with_menu(Box::new(tray_menu))
        .build()
        .expect("Failed to build tray icon");

    let mut badge_state = BadgeState::None;
    #[cfg(debug_assertions)]
    let mut badge_manual_override = false;

    let tray_channel = TrayIconEvent::receiver();
    let menu_channel = MenuEvent::receiver();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        #[cfg(target_os = "macos")]
        if let Event::NewEvents(StartCause::Init) = event {
            set_macos_dock_icon();
        }

        if let Ok(tray_event) = tray_channel.try_recv()
            && let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = tray_event
        {
            window.set_visible(true);
            window.set_focus();
        }

        if let Ok(menu_event) = menu_channel.try_recv() {
            #[cfg(debug_assertions)]
            {
                let forced = if menu_event.id == test_dot_id {
                    Some(BadgeState::Dot)
                } else if menu_event.id == test_count_id {
                    Some(BadgeState::Count(5))
                } else if menu_event.id == test_big_id {
                    Some(BadgeState::Count(150))
                } else if menu_event.id == test_clear_id {
                    Some(BadgeState::None)
                } else {
                    None
                };
                if let Some(state) = forced {
                    badge_manual_override = true;
                    badge_state = state;
                    apply_badge(&tray, state);
                } else if menu_event.id == test_resume_id {
                    badge_manual_override = false;
                }
            }

            if menu_event.id == quit_id {
                *control_flow = ControlFlow::Exit;
                return;
            }
        }

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                window.set_visible(false);
            }
            Event::UserEvent(UserEvent::DragWindow) => {
                let _ = window.drag_window();
            }
            Event::UserEvent(UserEvent::PageLoaded) => {
                if !window.is_visible() {
                    window.set_visible(true);
                    window.set_focus();
                }
            }
            Event::UserEvent(UserEvent::SetBadge(state)) => {
                #[cfg(debug_assertions)]
                if badge_manual_override {
                    return;
                }
                if state == badge_state {
                    return;
                }
                badge_state = state;
                apply_badge(&tray, state);
            }
            #[cfg(target_os = "macos")]
            Event::Reopen {
                has_visible_windows: false,
                ..
            } => {
                window.set_visible(true);
                window.set_focus();
            }
            _ => {}
        }
    });
}

#[cfg(target_os = "macos")]
fn set_macos_window_background(window: &tao::window::Window) {
    use objc2_app_kit::{
        NSAppearance, NSAppearanceCustomization, NSAppearanceNameDarkAqua, NSColor, NSView,
        NSWindow,
    };
    use tao::platform::macos::WindowExtMacOS;

    let ptr = window.ns_window() as *const NSWindow;
    if ptr.is_null() {
        return;
    }
    unsafe {
        let ns_window = &*ptr;
        let black = NSColor::blackColor();
        ns_window.setBackgroundColor(Some(&black));

        let dark = NSAppearance::appearanceNamed(NSAppearanceNameDarkAqua);
        ns_window.setAppearance(dark.as_deref());

        // Force the contentView to paint black before WKWebView renders its
        // first frame. Otherwise the default contentView shows a white flash
        // between window display and discord.com's first paint.
        if let Some(content_view) = ns_window.contentView() {
            let view: &NSView = &content_view;
            view.setWantsLayer(true);
            if let Some(layer) = view.layer() {
                let cg_black = black.CGColor();
                layer.setBackgroundColor(Some(&cg_black));
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn set_macos_dock_icon() {
    use objc2::AnyThread;
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSImage};
    use objc2_foundation::{MainThreadMarker, NSData};

    let bytes: &[u8] = include_bytes!("../icons/icon.icns");

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };

    let data = NSData::with_bytes(bytes);
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
        return;
    };

    let app = NSApplication::sharedApplication(mtm);
    unsafe {
        app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
        app.setApplicationIconImage(Some(&image));
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
    }
}

#[cfg(target_os = "macos")]
fn init_macos_app_menu() {
    let menu = Menu::new();

    let app_submenu = Submenu::new("Discord", true);
    app_submenu
        .append_items(&[
            &PredefinedMenuItem::services(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(None),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::quit(None),
        ])
        .unwrap();

    let edit_submenu = Submenu::new("Edit", true);
    edit_submenu
        .append_items(&[
            &PredefinedMenuItem::undo(None),
            &PredefinedMenuItem::redo(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::cut(None),
            &PredefinedMenuItem::copy(None),
            &PredefinedMenuItem::paste(None),
            &PredefinedMenuItem::select_all(None),
        ])
        .unwrap();

    let window_submenu = Submenu::new("Window", true);
    window_submenu
        .append_items(&[
            &PredefinedMenuItem::minimize(None),
            &PredefinedMenuItem::close_window(None),
        ])
        .unwrap();

    menu.append_items(&[&app_submenu, &edit_submenu, &window_submenu])
        .unwrap();

    menu.init_for_nsapp();
}

fn decode_rgba(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let img = image::load_from_memory(bytes).ok()?.into_rgba8();
    let (w, h) = (img.width(), img.height());
    Some((img.into_raw(), w, h))
}

// PNG decoding + the recolor/dot pixel passes only need to happen once. The JS
// observer fires on every Discord title change, so apply_badge can be hot.
fn cached_plain_rgba() -> Option<&'static (Vec<u8>, u32, u32)> {
    static CACHE: std::sync::OnceLock<Option<(Vec<u8>, u32, u32)>> = std::sync::OnceLock::new();
    CACHE
        .get_or_init(|| decode_rgba(include_bytes!("../icons/tray.png")))
        .as_ref()
}

fn cached_badged_rgba() -> Option<&'static (Vec<u8>, u32, u32)> {
    static CACHE: std::sync::OnceLock<Option<(Vec<u8>, u32, u32)>> = std::sync::OnceLock::new();
    CACHE
        .get_or_init(|| {
            let (mut rgba, w, h) = decode_rgba(include_bytes!("../icons/tray.png"))?;
            // Template mode flattens colors, so the badged variant ships a
            // non-template bitmap with a white silhouette (assumes dark menu
            // bar, the modern macOS default) and a red dot.
            for px in rgba.chunks_exact_mut(4) {
                if px[3] != 0 {
                    px[0] = 255;
                    px[1] = 255;
                    px[2] = 255;
                }
            }
            draw_red_dot(&mut rgba, w, h);
            Some((rgba, w, h))
        })
        .as_ref()
}

fn load_tray_icon_plain() -> Option<tray_icon::Icon> {
    let (rgba, w, h) = cached_plain_rgba()?;
    tray_icon::Icon::from_rgba(rgba.clone(), *w, *h).ok()
}

fn load_tray_icon_badged() -> Option<tray_icon::Icon> {
    let (rgba, w, h) = cached_badged_rgba()?;
    tray_icon::Icon::from_rgba(rgba.clone(), *w, *h).ok()
}

// Paints a solid red circle in the top-right of the buffer with a small
// transparent gap so the badge stays visually detached from the glyph.
fn draw_red_dot(buf: &mut [u8], w: u32, h: u32) {
    let radius = (h as f32 * 0.22).max(6.0);
    let gap = (h as f32 * 0.06).max(2.0);
    let cx = w as f32 - radius - 1.0;
    let cy = radius + 1.0;
    let bound = radius + gap + 1.0;
    let x_min = (cx - bound).max(0.0) as u32;
    let x_max = ((cx + bound).ceil() as u32).min(w);
    let y_min = (cy - bound).max(0.0) as u32;
    let y_max = ((cy + bound).ceil() as u32).min(h);
    for y in y_min..y_max {
        for x in x_min..x_max {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let idx = ((y * w + x) * 4) as usize;
            if dist <= radius {
                buf[idx] = 235;
                buf[idx + 1] = 64;
                buf[idx + 2] = 60;
                buf[idx + 3] = 255;
            } else if dist <= radius + gap {
                buf[idx + 3] = 0;
            }
        }
    }
}

fn apply_badge(tray: &tray_icon::TrayIcon, state: BadgeState) {
    let (icon, badged, label) = match state {
        BadgeState::None => (load_tray_icon_plain(), false, None),
        BadgeState::Dot => (load_tray_icon_badged(), true, Some("•".to_string())),
        BadgeState::Count(n) => (
            load_tray_icon_badged(),
            true,
            Some(if n > 99 { "99+".to_string() } else { n.to_string() }),
        ),
    };
    if let Some(icon) = icon {
        // The clean state stays template so macOS tints the silhouette to
        // match the menu bar; the badged state opts out of template mode so
        // the red dot keeps its color. We go through `set_icon_with_as_template`
        // because tray-icon 0.24's `set_icon` hardcodes `is_template = false`.
        let template = cfg!(target_os = "macos") && !badged;
        let _ = tray.set_icon_with_as_template(Some(icon), template);
    }

    #[cfg(target_os = "macos")]
    set_macos_dock_badge(label.as_deref());
    #[cfg(not(target_os = "macos"))]
    let _ = label;
}

#[cfg(target_os = "macos")]
fn set_macos_dock_badge(label: Option<&str>) {
    use objc2_app_kit::NSApplication;
    use objc2_foundation::{MainThreadMarker, NSString};

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    let dock = app.dockTile();
    let s = label.map(NSString::from_str);
    dock.setBadgeLabel(s.as_deref());
}

fn load_window_icon() -> Option<tao::window::Icon> {
    let (rgba, w, h) = decode_rgba(include_bytes!("../icons/icon.ico"))?;
    tao::window::Icon::from_rgba(rgba, w, h).ok()
}
