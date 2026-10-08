//! Modbus RTU Dashboard: finds, reads and configures Modbus RTU devices over RS485 (serial) or
//! the network (RS485<->Ethernet/WiFi converters, or native Modbus TCP).
//!
//! Without options it starts the local HTTP server on a free loopback port and shows the UI in a
//! native window (WebKitGTK on Linux, WebView2 on Windows, WKWebView on macOS), like ESPHome
//! Device Builder. `--serve` only runs the server, for a browser (the old modbus_dashboard.py).

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod core;
mod discovery;
mod jobs;
mod server;
mod transport;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

const APP_TITLE: &str = "Modbus RTU Dashboard";

#[derive(Parser)]
#[command(name = "modbus-dashboard", version, about = "Modbus RTU / TCP dashboard (RS485 and network devices).")]
struct Cli {
    /// Only run the HTTP server and open the UI in a browser instead of a window
    #[arg(long)]
    serve: bool,
    /// Listen address for --serve (127.0.0.1 = this machine only; 0.0.0.0 exposes it on the LAN)
    #[arg(long, default_value = "127.0.0.1", requires = "serve")]
    address: String,
    /// HTTP port for --serve
    #[arg(long, default_value_t = 6070, requires = "serve")]
    port: u16,
    /// With --serve: do not open a browser
    #[arg(long, requires = "serve")]
    no_browser: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = if cli.serve { serve(&cli) } else { window() };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn open_browser(url: &str) {
    #[cfg(target_os = "windows")]
    let cmd = ("cmd", vec!["/C", "start", "", url]);
    #[cfg(target_os = "macos")]
    let cmd = ("open", vec![url]);
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let cmd = ("xdg-open", vec![url]);
    if let Err(e) = std::process::Command::new(cmd.0).args(cmd.1).spawn() {
        eprintln!("Cannot open a browser: {e}");
    }
}

fn serve(cli: &Cli) -> Result<(), String> {
    let bound = server::start(&cli.address, cli.port, prefs_file())?;
    let host = if bound.ip().is_unspecified() { "localhost".to_string() } else { bound.ip().to_string() };
    let url = format!("http://{host}:{}/", bound.port());
    println!("Modbus Dashboard running at: {url}");
    println!("Stop: Ctrl+C");
    if !cli.no_browser {
        open_browser(&url);
    }
    loop {
        std::thread::park();
    }
}

/// UI preferences (language, theme), kept by the server: localStorage alone forgot them because
/// the window gets a new random port - a new origin - on every launch.
fn prefs_file() -> Option<PathBuf> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    #[cfg(target_os = "windows")]
    let base = env("APPDATA");
    #[cfg(target_os = "macos")]
    let base = env("HOME").map(|h| h.join("Library/Preferences"));
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let base = env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config")));
    base.map(|b| b.join("modbus-dashboard").join("prefs.json"))
}

/// Where the window keeps localStorage (language, theme) between runs.
fn data_dir() -> Option<PathBuf> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    #[cfg(target_os = "windows")]
    let base = env("LOCALAPPDATA");
    #[cfg(target_os = "macos")]
    let base = env("HOME").map(|h| h.join("Library/Application Support"));
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let base = env("XDG_DATA_HOME").or_else(|| env("HOME").map(|h| h.join(".local/share")));
    base.map(|b| b.join("modbus-dashboard"))
}

fn window() -> Result<(), String> {
    use tao::dpi::LogicalSize;
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoop};
    use tao::window::WindowBuilder;
    use wry::{WebContext, WebViewBuilder};

    // On Wayland the taskbar matches the window to modbus-dashboard.desktop (and its icon) by
    // app_id, which GTK takes from the program name; it must be set before GTK starts.
    #[cfg(target_os = "linux")]
    gtk::glib::set_prgname(Some("modbus-dashboard"));

    let bound = server::start("127.0.0.1", 0, prefs_file())?;
    let url = format!("http://127.0.0.1:{}/", bound.port());

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title(APP_TITLE)
        .with_inner_size(LogicalSize::new(1000.0, 800.0))
        .with_min_inner_size(LogicalSize::new(760.0, 600.0))
        .build(&event_loop)
        .map_err(|e| format!("Cannot create the window: {e}"))?;

    // A persistent data directory: without it WebKitGTK may run without localStorage, and the UI
    // keeps its language and theme there.
    let mut context = WebContext::new(data_dir());
    let builder = WebViewBuilder::new_with_web_context(&mut context).with_url(&url);

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    let webview = builder.build(&window);
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        let vbox = window.default_vbox().ok_or("The window has no GTK container")?;
        builder.build_gtk(vbox)
    };
    let _webview = webview.map_err(|e| format!("Cannot create the web view: {e}"))?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent { event: WindowEvent::CloseRequested, .. } = event {
            *control_flow = ControlFlow::Exit;
        }
    });
}
