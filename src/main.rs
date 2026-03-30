use muda::{Menu, MenuEvent, MenuItem};
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    platform::unix::WindowExtUnix,
    window::WindowBuilder,
};
use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use wry::{WebViewBuilder, WebViewBuilderExtUnix};

fn main() -> wry::Result<()> {
    unsafe { std::env::set_var("GDK_BACKEND", "wayland") };

    let event_loop = EventLoop::new();

    let window = WindowBuilder::new()
        .with_title("Discord")
        .with_visible(false)
        .with_inner_size(tao::dpi::LogicalSize::new(800.0, 600.0))
        .build(&event_loop)
        .expect("Failed to build window");

    #[cfg(target_os = "windows")]
    window_shadows::set_shadow(&window, true).ok();

    window.set_visible(true);

    let vbox = window.default_vbox().expect("Failed to get vbox");
    let _webview = WebViewBuilder::new()
        .with_html("<script>window.location.replace('https://discord.com/app')</script>")
        .build_gtk(vbox)
        .expect("Failed to build WebView");

    let tray_menu = Menu::new();
    let quit_item = MenuItem::new("Quit", true, None);
    tray_menu.append(&quit_item).unwrap();
    let quit_id = quit_item.id().clone();

    let _tray = TrayIconBuilder::new()
        .with_tooltip("Discord")
        .with_menu(Box::new(tray_menu))
        .build()
        .expect("Failed to build tray icon");

    let tray_channel = TrayIconEvent::receiver();
    let menu_channel = MenuEvent::receiver();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        // left click on tray icon windows and mac only
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
