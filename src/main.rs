#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use image::GenericImageView;
use muda::{Menu, MenuEvent, MenuItem};
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use wry::WebViewBuilder;

#[cfg(target_family = "unix")]
use tao::platform::unix::WindowExtUnix;
#[cfg(target_family = "unix")]
use wry::WebViewBuilderExtUnix;

fn main() -> wry::Result<()> {
    #[cfg(target_family = "unix")]
    unsafe {
        std::env::set_var("GDK_BACKEND", "wayland")
    };

    let event_loop = EventLoop::new();

    let window = WindowBuilder::new()
        .with_title("Discord")
        .with_visible(false)
        .with_window_icon(load_window_icon())
        .with_inner_size(tao::dpi::LogicalSize::new(800.0, 600.0))
        .build(&event_loop)
        .expect("Failed to build window");

    window.set_visible(true);

    let html = "<script>window.location.replace('https://discord.com/app')</script>";

    #[cfg(target_os = "linux")]
    let _webview = {
        let vbox = window.default_vbox().expect("Failed to get vbox");
        WebViewBuilder::new()
            .with_html(html)
            .build_gtk(vbox)
            .expect("Failed to build WebView")
    };

    #[cfg(not(target_os = "linux"))]
    let _webview = WebViewBuilder::new()
        .with_html(html)
        .build(&window)
        .expect("Failed to build WebView");

    let tray_menu = Menu::new();
    let quit_item = MenuItem::new("Quit", true, None);
    tray_menu.append(&quit_item).unwrap();
    let quit_id = quit_item.id().clone();

    let _tray = TrayIconBuilder::new()
        .with_icon(load_tray_icon().expect("Failed to load icon"))
        .with_tooltip("Discord")
        .with_menu(Box::new(tray_menu))
        .build()
        .expect("Failed to build tray icon");

    let tray_channel = TrayIconEvent::receiver();
    let menu_channel = MenuEvent::receiver();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        if let Ok(tray_event) = tray_channel.try_recv()
            && let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = tray_event
        {
            if !window.is_visible() {
                window.set_visible(true);
            } else {
                window.set_focus();
            }
        }

        if let Ok(menu_event) = menu_channel.try_recv()
            && menu_event.id == quit_id
        {
            std::process::exit(0);
        }

        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            window.set_visible(false);
        }
    });
}

fn load_tray_icon() -> Option<tray_icon::Icon> {
    let img = image::load_from_memory(include_bytes!("../icons/tray.ico")).ok()?;
    let (width, height) = img.dimensions();

    tray_icon::Icon::from_rgba(img.to_rgba8().into_raw(), width, height).ok()
}

fn load_window_icon() -> Option<tao::window::Icon> {
    let img = image::load_from_memory(include_bytes!("../icons/icon.ico")).ok()?;
    let (width, height) = img.dimensions();

    tao::window::Icon::from_rgba(img.to_rgba8().into_raw(), width, height).ok()
}
