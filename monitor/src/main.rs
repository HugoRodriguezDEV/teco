//! Teco Monitor 🦉: el tablero de toda la red, siempre a la mano.
//!
//! - Vive en la barra de menú (macOS) o en la bandeja (Windows), sin ícono en el Dock.
//! - El tecolote cambia de color con el peor estado de la red: normal, amarillo (atento),
//!   rojo (alerta) o gris con punto rojo (sin Tailscale / sin Teco).
//! - "Abrir tablero" muestra la vista de Dispositivos en una ventana propia. Al cerrarla se
//!   libera el motor web; el tecolote y los avisos siguen.
//! - Vigila Tailscale: si la app de Tailscale se cerró, la vuelve a abrir en segundo plano.
//!
//! Es un proceso aparte del vigilante (`teco`), que sigue con su presupuesto intacto.

#![cfg_attr(windows, windows_subsystem = "windows")]

use serde::Deserialize;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::window::{Window, WindowBuilder};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent};
use wry::{WebView, WebViewBuilder};

/// Panel local del vigilante (vista de Dispositivos en `/`).
const LOCAL: &str = "http://127.0.0.1:7777";
/// Puerto para "una sola instancia": abrir la app otra vez solo trae la ventana al frente.
const SINGLE: &str = "127.0.0.1:7789";
const POLL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Level {
    Ok,
    Watch,
    Alert,
    /// Sin Tailscale o sin el vigilante local: no se puede ver la red.
    Off,
}

#[derive(Debug, Clone)]
struct Fleet {
    level: Level,
    line: String,
}

enum UserEvent {
    Fleet(Fleet),
    Menu(MenuEvent),
    Tray(TrayIconEvent),
    Open,
}

#[derive(Deserialize)]
struct Summary {
    #[serde(default)]
    status: String,
    #[serde(default)]
    host: String,
}

#[derive(Deserialize)]
struct Device {
    name: String,
    url: String,
    #[serde(default)]
    auto: bool,
    #[serde(default)]
    teco: bool,
    #[serde(default = "yes")]
    online: bool,
}

fn yes() -> bool {
    true
}

fn main() {
    // Una sola instancia: si ya corre, se le pide que abra la ventana y salimos.
    let single = match TcpListener::bind(SINGLE) {
        Ok(l) => l,
        Err(_) => {
            if let Ok(mut s) = TcpStream::connect(SINGLE) {
                let _ = s.write_all(b"open\n");
            }
            return;
        }
    };
    let tray_only = std::env::args().any(|a| a == "--tray-only");

    let mut event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    #[cfg(target_os = "macos")]
    {
        use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
        // Solo barra de menú: sin ícono en el Dock ni en Cmd+Tab.
        event_loop.set_activation_policy(ActivationPolicy::Accessory);
    }
    let proxy = event_loop.create_proxy();

    {
        let p = proxy.clone();
        MenuEvent::set_event_handler(Some(move |e| {
            let _ = p.send_event(UserEvent::Menu(e));
        }));
        let p = proxy.clone();
        TrayIconEvent::set_event_handler(Some(move |e| {
            let _ = p.send_event(UserEvent::Tray(e));
        }));
    }
    spawn_single_listener(single, proxy.clone());
    spawn_poller(proxy.clone());

    let menu = Menu::new();
    let status_item = MenuItem::new("Revisando la red…", false, None);
    let open_item = MenuItem::new("Abrir tablero", true, None);
    let local_item = MenuItem::new("Procesos de este equipo", true, None);
    let quit_item = MenuItem::new("Salir del monitor", true, None);
    let _ = menu.append(&status_item);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&open_item);
    let _ = menu.append(&local_item);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&quit_item);

    let mut tray: Option<TrayIcon> = None;
    let mut window: Option<(Window, WebView)> = None;
    let mut current = Level::Off;

    event_loop.run(move |event, target, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::NewEvents(StartCause::Init) => {
                // En macOS el ícono debe crearse ya con el bucle corriendo.
                let b = TrayIconBuilder::new().with_menu(Box::new(menu.clone())).with_tooltip("Teco");
                // macOS: el tecolote normal es "plantilla" (se adapta a la barra clara u oscura).
                #[cfg(target_os = "macos")]
                let b = b.with_icon_templated(icon_for(Level::Ok));
                #[cfg(not(target_os = "macos"))]
                let b = b.with_icon(icon_for(Level::Ok));
                tray = b.build().ok();
                current = Level::Ok;
                if !tray_only {
                    let _ = proxy.send_event(UserEvent::Open);
                }
            }
            Event::UserEvent(UserEvent::Fleet(f)) => {
                status_item.set_text(&f.line);
                if let Some(t) = &tray {
                    let _ = t.set_tooltip(Some(format!("Teco · {}", f.line)));
                    if f.level != current {
                        set_tray_icon(t, f.level);
                        current = f.level;
                    }
                }
            }
            Event::UserEvent(UserEvent::Open) => open_window(&mut window, target, "/"),
            Event::UserEvent(UserEvent::Menu(e)) => {
                if e.id == *open_item.id() {
                    open_window(&mut window, target, "/");
                } else if e.id == *local_item.id() {
                    open_window(&mut window, target, "/equipo");
                } else if e.id == *quit_item.id() {
                    window = None;
                    tray = None;
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::UserEvent(UserEvent::Tray(TrayIconEvent::DoubleClick { button: MouseButton::Left, .. })) => {
                // Windows: doble clic abre el tablero (en macOS el clic abre el menú).
                open_window(&mut window, target, "/");
            }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                // Cerrar la ventana libera el motor web; el monitor sigue en la barra.
                window = None;
            }
            _ => {}
        }
    });
}

fn open_window(slot: &mut Option<(Window, WebView)>, target: &tao::event_loop::EventLoopWindowTarget<UserEvent>, path: &str) {
    let url = format!("{LOCAL}{path}");
    if let Some((w, v)) = slot.as_ref() {
        if path != "/" {
            let _ = v.load_url(&url);
        }
        w.set_visible(true);
        w.set_minimized(false);
        w.set_focus();
        focus_app(target);
        return;
    }
    let w = match WindowBuilder::new()
        .with_title("Teco")
        .with_inner_size(tao::dpi::LogicalSize::new(1120.0, 780.0))
        .with_min_inner_size(tao::dpi::LogicalSize::new(380.0, 400.0))
        .build(target)
    {
        Ok(w) => w,
        Err(e) => {
            eprintln!("teco-monitor: no pude crear la ventana: {e}");
            return;
        }
    };
    match WebViewBuilder::new().with_url(&url).build(&w) {
        Ok(v) => {
            w.set_focus();
            focus_app(target);
            *slot = Some((w, v));
        }
        Err(e) => eprintln!("teco-monitor: no pude crear la vista web: {e}"),
    }
}

#[cfg(target_os = "macos")]
fn focus_app(target: &tao::event_loop::EventLoopWindowTarget<UserEvent>) {
    use tao::platform::macos::EventLoopWindowTargetExtMacOS;
    // Una app de barra de menú no pasa sola al frente: hay que pedirlo.
    target.show_application();
}
#[cfg(not(target_os = "macos"))]
fn focus_app(_: &tao::event_loop::EventLoopWindowTarget<UserEvent>) {}

fn set_tray_icon(t: &TrayIcon, level: Level) {
    let icon = Some(icon_for(level));
    #[cfg(target_os = "macos")]
    let _ = if level == Level::Ok { t.set_icon_templated(icon) } else { t.set_icon(icon) };
    #[cfg(not(target_os = "macos"))]
    let _ = t.set_icon(icon);
}

fn icon_for(level: Level) -> Icon {
    let bytes: &[u8] = match level {
        Level::Ok if cfg!(target_os = "macos") => include_bytes!("../icons/tray-ok.png"),
        Level::Ok => include_bytes!("../icons/tray-ok-gray.png"),
        Level::Watch => include_bytes!("../icons/tray-watch.png"),
        Level::Alert => include_bytes!("../icons/tray-alert.png"),
        Level::Off => include_bytes!("../icons/tray-off.png"),
    };
    let (rgba, w, h) = decode_png(bytes);
    Icon::from_rgba(rgba, w, h).expect("ícono válido")
}

fn decode_png(bytes: &[u8]) -> (Vec<u8>, u32, u32) {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
    let mut reader = dec.read_info().expect("png");
    let mut buf = vec![0; reader.output_buffer_size().expect("tamaño png")];
    let info = reader.next_frame(&mut buf).expect("frame png");
    buf.truncate(info.buffer_size());
    (buf, info.width, info.height)
}

fn spawn_single_listener(l: TcpListener, proxy: EventLoopProxy<UserEvent>) {
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            let mut s = s;
            let _ = s.set_read_timeout(Some(Duration::from_secs(1)));
            let mut b = [0u8; 8];
            if matches!(s.read(&mut b), Ok(n) if n > 0) {
                let _ = proxy.send_event(UserEvent::Open);
            }
        }
    });
}

// ------------------------------------------------------------------ red

fn spawn_poller(proxy: EventLoopProxy<UserEvent>) {
    std::thread::spawn(move || {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(3)))
            .http_status_as_error(true)
            .build()
            .into();
        let mut last_relaunch: Option<Instant> = None;
        loop {
            let f = check(&agent, &mut last_relaunch);
            if proxy.send_event(UserEvent::Fleet(f)).is_err() {
                return; // el bucle principal terminó
            }
            std::thread::sleep(POLL);
        }
    });
}

fn get<T: for<'de> Deserialize<'de>>(agent: &ureq::Agent, url: &str) -> Option<T> {
    let mut r = agent.get(url).call().ok()?;
    let body = r.body_mut().read_to_string().ok()?;
    serde_json::from_str(&body).ok()
}

/// Revisa Tailscale, el vigilante local y cada nodo; devuelve el peor estado.
fn check(agent: &ureq::Agent, last_relaunch: &mut Option<Instant>) -> Fleet {
    let ts = tailscale_state(last_relaunch);
    let Some(local) = get::<Summary>(agent, &format!("{LOCAL}/api/summary")) else {
        return Fleet { level: Level::Off, line: "Teco no está corriendo en este equipo".into() };
    };
    let mut level = level_of(&local.status);
    let mut problems: Vec<String> = Vec::new();
    if level >= Level::Watch {
        problems.push(local.host.clone());
    }
    let mut total = 1;
    if ts != TsState::Running {
        let msg = match ts {
            TsState::Missing => "Tailscale no está instalado",
            TsState::Stopped => "Tailscale está desconectado",
            TsState::Running => unreachable!(),
        };
        return Fleet { level: Level::Off, line: format!("{msg} · solo se ve este equipo") };
    }
    let devices: Vec<Device> = get(agent, &format!("{LOCAL}/api/devices")).unwrap_or_default();
    for d in devices {
        // Descubiertos sin tag:teco: si no responden, no cuentan (puede que no tengan Teco).
        let expected = !d.auto || d.teco;
        if !d.online {
            if expected {
                total += 1;
                level = level.max(Level::Alert);
                problems.push(format!("{} sin conexión", d.name));
            }
            continue;
        }
        match get::<Summary>(agent, &format!("{}/api/summary", d.url)) {
            Some(s) => {
                total += 1;
                let l = level_of(&s.status);
                if l >= Level::Watch {
                    problems.push(d.name.clone());
                }
                level = level.max(l);
            }
            None if expected => {
                total += 1;
                level = level.max(Level::Alert);
                problems.push(format!("{} no responde", d.name));
            }
            None => {}
        }
    }
    let word = match level {
        Level::Ok => "Tranquilo",
        Level::Watch => "Atento",
        Level::Alert => "Alerta",
        Level::Off => "Sin conexión",
    };
    let equipos = if total == 1 { "1 equipo".to_string() } else { format!("{total} equipos") };
    let line = if problems.is_empty() {
        format!("{word} · {equipos}")
    } else {
        format!("{word} · {}", problems.join(", "))
    };
    Fleet { level, line }
}

fn level_of(s: &str) -> Level {
    match s {
        "alert" => Level::Alert,
        "watch" => Level::Watch,
        _ => Level::Ok,
    }
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum TsState {
    Running,
    Stopped,
    Missing,
}

/// Estado de Tailscale. En macOS, si la app de Tailscale está cerrada, la vuelve a abrir en
/// segundo plano (como mucho una vez por minuto). No reconecta si tú la desconectaste.
fn tailscale_state(last_relaunch: &mut Option<Instant>) -> TsState {
    #[cfg(target_os = "macos")]
    {
        let app = std::path::Path::new("/Applications/Tailscale.app");
        if !app.exists() {
            return TsState::Missing;
        }
        let running = Command::new("/usr/bin/pgrep")
            .args(["-xq", "Tailscale"])
            .stdin(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(true);
        if !running && last_relaunch.map(|t| t.elapsed() > Duration::from_secs(60)).unwrap_or(true) {
            *last_relaunch = Some(Instant::now());
            // -g: sin pasar al frente; -j: oculta.
            let _ = Command::new("/usr/bin/open").args(["-gj", "-a", "Tailscale"]).status();
        }
        let out = Command::new("/Applications/Tailscale.app/Contents/MacOS/Tailscale")
            .args(["status", "--json"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output();
        return backend_state(out);
    }
    #[cfg(windows)]
    {
        let _ = last_relaunch;
        use std::os::windows::process::CommandExt;
        let exe = r"C:\Program Files\Tailscale\tailscale.exe";
        if !std::path::Path::new(exe).exists() {
            return TsState::Missing;
        }
        let out = Command::new(exe)
            .args(["status", "--json"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .output();
        return backend_state(out);
    }
    #[allow(unreachable_code)]
    {
        let _ = last_relaunch;
        backend_state(Command::new("tailscale").args(["status", "--json"]).stderr(Stdio::null()).output())
    }
}

fn backend_state(out: std::io::Result<std::process::Output>) -> TsState {
    #[derive(Deserialize)]
    struct S {
        #[serde(rename = "BackendState", default)]
        state: String,
    }
    match out {
        Err(_) => TsState::Missing,
        Ok(o) => match serde_json::from_slice::<S>(&o.stdout) {
            Ok(s) if s.state == "Running" => TsState::Running,
            _ => TsState::Stopped,
        },
    }
}
