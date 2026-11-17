//! Teco 🦉 — guardián de recursos ultraligero.
//! Callado, mira toda la noche y solo hace ruido cuando hay algo.

// En Windows corre sin ventana de consola; los subcomandos se enganchan a la consola padre.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod config;
mod events;
mod monitor;
mod notify;
mod server;
mod tailnet;
mod util;

use config::Config;
use events::{EventLog, Kind};
use monitor::Monitor;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const HELP: &str = "\
teco 🦉 — guardián de recursos ultraligero

USO:
  teco [run] [--config <ruta>]     vigila en segundo plano y sirve el panel
  teco bench [--secs N] [--config <ruta>]
                                   muestrea N s (60) y mide RSS/CPU propios contra el presupuesto
  teco check [--config <ruta>]     valida la config y muestra las reglas
  teco config-path                 imprime dónde está teco.toml
  teco --version | --help
";

struct Args {
    cmd: String,
    config: Option<PathBuf>,
    secs: u64,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args { cmd: "run".into(), config: None, secs: 60 };
    let mut it = std::env::args().skip(1);
    let mut cmd_set = false;
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-c" | "--config" => a.config = Some(it.next().ok_or("--config necesita una ruta")?.into()),
            "--secs" => {
                a.secs = it.next().and_then(|v| v.parse().ok()).ok_or("--secs necesita un número")?;
            }
            "-h" | "--help" | "help" => a.cmd = "help".into(),
            "-V" | "--version" | "version" => a.cmd = "version".into(),
            s if !cmd_set && !s.starts_with('-') => {
                a.cmd = s.into();
                cmd_set = true;
            }
            s => return Err(format!("argumento desconocido: {s}")),
        }
    }
    Ok(a)
}

fn main() {
    let t0 = Instant::now();
    attach_console();
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("teco: {e}\n\n{HELP}");
            std::process::exit(2);
        }
    };
    let path = args.config.clone().unwrap_or_else(config::default_path);
    match args.cmd.as_str() {
        "help" => print!("{HELP}"),
        "version" => println!("teco {}", env!("CARGO_PKG_VERSION")),
        "config-path" => println!("{}", path.display()),
        "check" => check(&path),
        "run" => run(&path, t0),
        "bench" => bench(&path, args.secs, t0),
        other => {
            eprintln!("teco: comando desconocido: {other}\n\n{HELP}");
            std::process::exit(2);
        }
    }
}

fn load(path: &Path) -> Config {
    config::load_or_create(path).unwrap_or_else(|e| {
        eprintln!("teco: config inválida: {e}");
        std::process::exit(1);
    })
}

fn check(path: &Path) {
    let c = load(path);
    println!("✓ {}", path.display());
    println!("  panel: http://{}  · cada {} s · terminar desde el panel: {}", c.bind, c.interval_secs, if c.allow_kill_from_panel { "sí" } else { "no" });
    let s = &c.system;
    let sys = limits(&[("RAM", s.max_mem_percent, "%"), ("swap", s.max_swap_gb, "GB"), ("CPU", s.max_cpu_percent, "%"), ("GPU", s.max_gpu_percent, "%"), ("VRAM", s.max_vram_percent, "%")]);
    println!("  sistema: {sys} · sostenido {} s", s.for_secs);
    for r in &c.rules {
        let lims = limits(&[("mem", r.max_mem_gb, "GB"), ("cpu", r.max_cpu_percent, "%"), ("vivo", r.max_runtime_min, "min"), ("gpu", r.max_gpu_percent, "%"), ("vram", r.max_vram_gb, "GB")]);
        let action = match r.action {
            config::Action::Alert => "avisar",
            config::Action::Kill => "terminar",
        };
        println!(
            "  • {} — match {:?}{} · {lims} · sostenido {} s · {action}",
            r.name,
            r.pattern,
            if r.include_children { " (+hijos)" } else { "" },
            r.for_secs,
        );
    }
}

/// "RAM 90 % · CPU 95 %": only the limits that are set; "sin límites" if none.
fn limits(items: &[(&str, Option<f64>, &str)]) -> String {
    let parts: Vec<String> = items
        .iter()
        .filter_map(|&(label, v, unit)| v.map(|v| format!("{label} {} {unit}", num(v))))
        .collect();
    if parts.is_empty() { "sin límites".into() } else { parts.join(" · ") }
}

/// 90.0 → "90", 1.5 → "1.5".
fn num(v: f64) -> String {
    if v.fract() == 0.0 { format!("{v:.0}") } else { format!("{v}") }
}

fn run(path: &Path, t0: Instant) {
    let cfg = load(path);
    let log_path = path.parent().map(|d| d.join("teco-events.log"));
    let bind = cfg.bind.clone();
    let mut gpu = monitor::GpuProbe::detect();
    let mut mon = Monitor::new(cfg, EventLog::new(log_path), gpu.is_some());
    let listener = match server::bind(&bind) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("teco: no pude escuchar en {bind}: {e} (¿otro teco ya corre?)");
            std::process::exit(1);
        }
    };
    let tailnet = mon.cfg.bind_tailnet.trim().to_string();
    let tailnet_listener = if tailnet.is_empty() {
        None
    } else if tailnet == bind {
        eprintln!("teco: bind y bind_tailnet no pueden ser la misma dirección ({bind})");
        std::process::exit(1);
    } else {
        match server::bind(&tailnet) {
            Ok(l) => Some(l),
            Err(e) => {
                eprintln!("teco: no pude escuchar en {tailnet}: {e}");
                std::process::exit(1);
            }
        }
    };
    if let Some(g) = gpu.as_mut() {
        mon.set_gpu(g.read());
    }
    mon.tick();
    mon.events.push(
        Kind::Info,
        "Teco despierto",
        format!(
            "panel en http://{bind}{} · config {} · arranque {} ms",
            if tailnet.is_empty() { String::new() } else { format!(" · tailnet → {tailnet}") },
            path.display(),
            t0.elapsed().as_millis()
        ),
        0,
    );
    let mon = Arc::new(Mutex::new(mon));
    if let Err(e) = server::spawn(listener, mon.clone(), server::Via::Local) {
        eprintln!("teco: no pude iniciar el panel: {e}");
        std::process::exit(1);
    }
    if let Some(l) = tailnet_listener {
        if let Err(e) = server::spawn(l, mon.clone(), server::Via::Tailnet) {
            eprintln!("teco: no pude iniciar el listener de la tailnet: {e}");
            std::process::exit(1);
        }
    }
    eprintln!("teco 🦉 vigilando · http://{bind} · {}", path.display());
    sample_loop(&mon, path, None, &mut gpu);
}

/// Bucle de muestreo. Con `until`, termina en ese instante (bench).
/// `gpu` se lee aquí, fuera del candado: solo este hilo toca la tarjeta.
fn sample_loop(mon: &Mutex<Monitor>, path: &Path, until: Option<Instant>, gpu: &mut Option<monitor::GpuProbe>) {
    let mut cfg_mtime = config::mtime(path);
    let mut next = Instant::now();
    loop {
        let interval = Duration::from_secs(lock(mon).cfg.interval_secs);
        next += interval;
        let now = Instant::now();
        if next <= now {
            next = now + interval; // nos atrasamos (p. ej. tras suspender): no ráfagas
        }
        if let Some(u) = until {
            if next > u {
                std::thread::sleep(u.saturating_duration_since(now));
                return;
            }
        }
        std::thread::sleep(next - now);

        // Recarga en caliente: un `stat` por muestra.
        let mt = config::mtime(path);
        if mt != cfg_mtime {
            cfg_mtime = mt;
            reload(mon, path);
        }
        let snap = gpu.as_mut().map(|g| g.read());
        let mut m = lock(mon);
        if let Some(s) = snap {
            m.set_gpu(s);
        }
        m.tick();
    }
}

fn reload(mon: &Mutex<Monitor>, path: &Path) {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return, // a veces el editor borra y reescribe: lo intentamos en la próxima
    };
    let mut m = lock(mon);
    match Config::parse(&text) {
        Ok(c) => {
            let note = if c.bind != m.cfg.bind || c.bind_tailnet != m.cfg.bind_tailnet { " (bind cambia al reiniciar)" } else { "" };
            let n = c.rules.len();
            m.set_config(c);
            m.events.push(Kind::Info, "Config recargada", format!("{n} reglas{note}"), 0);
        }
        Err(e) => m.events.push(Kind::Error, "Config inválida", format!("se mantiene la anterior: {e}"), 0),
    }
}

fn lock(m: &Mutex<Monitor>) -> std::sync::MutexGuard<'_, Monitor> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn bench(path: &Path, secs: u64, t0: Instant) {
    let cfg = load(path);
    let interval = cfg.interval_secs;
    let mut gpu = monitor::GpuProbe::detect();
    let mut mon = Monitor::new(cfg, EventLog::new(None), gpu.is_some());
    mon.dry_run = true;
    mon.tick();
    let startup = t0.elapsed();
    // El panel también escucha (puerto efímero) para medir su costo en reposo.
    let listener = server::bind("127.0.0.1:0").expect("puerto efímero");
    let addr = listener.local_addr().map(|a| a.to_string()).unwrap_or_default();
    let mon = Arc::new(Mutex::new(mon));
    server::spawn(listener, mon.clone(), server::Via::Local).expect("panel");

    println!("teco bench 🦉 · {secs} s · muestreo cada {interval} s · panel cerrado en http://{addr}");
    let cpu0 = own_cpu_ms();
    let w0 = Instant::now();
    let mut peak = 0u64;
    let end = w0 + Duration::from_secs(secs);
    let mut last_print = Instant::now();
    while Instant::now() < end {
        let step_end = (Instant::now() + Duration::from_secs(10)).min(end);
        sample_loop(&mon, path, Some(step_end), &mut gpu);
        let rss = own_rss();
        peak = peak.max(rss);
        if last_print.elapsed() >= Duration::from_secs(10) || Instant::now() >= end {
            let m = lock(&mon);
            println!(
                "  {:>3} s · {} procesos · RSS {:.1} MB",
                w0.elapsed().as_secs(),
                m.rows.len(),
                rss as f64 / 1048576.0
            );
            last_print = Instant::now();
        }
    }
    let wall = w0.elapsed().as_secs_f64();
    let cpu_ms = own_cpu_ms().saturating_sub(cpu0);
    let cpu_pct = cpu_ms as f64 / 10.0 / wall;
    let rss = own_rss();
    peak = peak.max(rss);
    let bin = std::env::current_exe().and_then(std::fs::metadata).map(|m| m.len()).unwrap_or(0);

    let ok = |b: bool| if b { "✅" } else { "❌" };
    let mb = |b: u64| b as f64 / 1048576.0;
    println!();
    println!("  {} RAM (RSS) en reposo   {:>7.1} MB  (pico {:.1} MB, límite 15 MB)", ok(mb(rss) < 15.0), mb(rss), mb(peak));
    println!("  {} CPU promedio          {:>7.3} %   ({} ms en {:.0} s, límite 0.5 %)", ok(cpu_pct < 0.5), cpu_pct, cpu_ms, wall);
    println!("  {} Binario               {:>7.1} MB  (límite 6 MB)", ok(mb(bin) < 6.0), mb(bin));
    println!("  {} Arranque              {:>7} ms  (hasta la primera muestra, límite 200 ms)", ok(startup.as_millis() < 200), startup.as_millis());
    if cfg!(debug_assertions) {
        println!("\n  ⚠️  build de depuración: mide con `cargo run --release -- bench`.");
    }
}

/// RSS y CPU acumulada propios, leídos con un `System` aparte que solo mira nuestro PID.
fn own_proc<T>(f: impl Fn(&sysinfo::Process) -> T) -> Option<T> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    let pid = Pid::from_u32(std::process::id());
    let mut s = System::new().ok()?;
    s.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        false,
        ProcessRefreshKind::nothing().with_memory().with_cpu(),
    );
    s.process(pid).map(f)
}
fn own_rss() -> u64 {
    own_proc(|p| p.memory()).unwrap_or(0)
}
fn own_cpu_ms() -> u64 {
    own_proc(|p| p.accumulated_cpu_time()).unwrap_or(0)
}

#[cfg(windows)]
fn attach_console() {
    use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    // Si nos lanzaron desde una terminal, escribimos en ella; si no (Programador de tareas), nada.
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}
#[cfg(not(windows))]
fn attach_console() {}
