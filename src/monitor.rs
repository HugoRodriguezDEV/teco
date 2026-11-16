//! Núcleo: muestreo con `sysinfo`, motor de reglas y acciones.
//!
//! Todo vive en una sola estructura que se reutiliza entre muestras:
//! - `System` se crea una vez.
//! - Nombre y comando se leen una sola vez por PID (`UpdateKind::OnlyIfNotSet`)
//!   y se guardan como `Arc<str>`; en el ciclo caliente solo se incrementa un contador.
//! - Qué reglas aplican a cada proceso se calcula una vez por PID (bitmask `u64`)
//!   y solo se recalcula cuando cambia la configuración.
//! - El JSON del panel se arma únicamente cuando alguien lo pide.

use crate::config::{Action, Config};
use crate::events::{EventLog, Kind};
use crate::notify;
use crate::util::{fmt_bytes, fmt_dur, json_num, json_str, now_ms, truncate, GB};
use std::collections::HashMap;
use std::fmt::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

const CMD_MAX_CHARS: usize = 300;
const WATCH_RATIO: f64 = 0.8;
const MAX_TREE_DEPTH: usize = 64;
const KILL_GRACE: Duration = Duration::from_secs(5);
/// Claves "de sistema" en la tabla de seguimiento (no chocan con índices de regla).
const SYS_MEM: u16 = u16::MAX;
const SYS_SWAP: u16 = u16::MAX - 1;
const SYS_CPU: u16 = u16::MAX - 2;

struct Meta {
    start: u64,
    name: Arc<str>,
    cmd: Arc<str>,
    /// App dueña, sacada de la ruta del ejecutable (`/Applications/X.app/...` → "X").
    app: Option<Arc<str>>,
    /// nombre + comando en minúsculas, para el `match` de las reglas.
    hay: Box<str>,
    mask: u64,
    mask_gen: u32,
    seen: u32,
}

#[derive(Clone)]
pub struct Row {
    pub pid: u32,
    pub ppid: u32,
    pub start: u64,
    pub cpu: f32,
    pub mem: u64,
    pub runtime: u64,
    pub name: Arc<str>,
    pub cmd: Arc<str>,
    mask: u64,
    /// Máxima proporción valor/límite entre las reglas que le aplican (≥0.8 atento, ≥1 pasado).
    pub heat: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Metric {
    Mem,
    Cpu,
    Runtime,
    SysMem,
    SysSwap,
    SysCpu,
}

impl Metric {
    fn key(self) -> &'static str {
        match self {
            Metric::Mem => "mem",
            Metric::Cpu => "cpu",
            Metric::Runtime => "runtime",
            Metric::SysMem => "sys_mem",
            Metric::SysSwap => "sys_swap",
            Metric::SysCpu => "sys_cpu",
        }
    }
    /// Texto humano de un valor en las unidades de la métrica.
    fn fmt(self, v: f64) -> String {
        match self {
            Metric::Mem | Metric::SysSwap => format!("{v:.1} GB"),
            Metric::Cpu | Metric::SysCpu | Metric::SysMem => format!("{v:.0} %"),
            Metric::Runtime => fmt_dur((v * 60.0) as u64),
        }
    }
    fn fmt_limit(self, v: f64) -> String {
        match self {
            Metric::Mem | Metric::SysSwap => format!("{} GB", trim_num(v)),
            Metric::Cpu | Metric::SysCpu | Metric::SysMem => format!("{} %", trim_num(v)),
            Metric::Runtime => format!("{} min", trim_num(v)),
        }
    }
}

fn trim_num(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct Key {
    rule: u16,
    pid: u32,
    start: u64,
}

struct Track {
    over_since: Option<Instant>,
    under_since: Option<Instant>,
    /// Hora (Unix ms) en que empezó a pasarse, para el panel.
    over_since_ms: u64,
    fired: bool,
    fired_at: Option<Instant>,
    fired_ms: u64,
    seen: u32,
    name: Arc<str>,
    rule_name: Arc<str>,
    metric: Metric,
    value: f64,
    limit: f64,
    ratio: f64,
    mem: u64,
    members: u32,
}

/// Un candidato a evaluar: un proceso, un árbol agrupado o una métrica del sistema.
struct Cand {
    key: Key,
    name: Arc<str>,
    metric: Metric,
    value: f64,
    limit: f64,
    ratio: f64,
    mem: u64,
    members: u32,
}

enum Step {
    Nothing,
    Fire { refire: bool },
    Resolved,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    Watch,
    Alert,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Watch => "watch",
            Status::Alert => "alert",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortBy {
    Mem,
    Cpu,
}

pub struct Monitor {
    sys: System,
    refresh: ProcessRefreshKind,
    pub cfg: Config,
    cfg_gen: u32,
    needles: Vec<Option<String>>,
    cache: HashMap<u32, Meta>,
    pub rows: Vec<Row>,
    idx: HashMap<u32, u32>,
    agg: HashMap<u32, (u64, f32, u32)>,
    root_of: Vec<u32>,
    tracks: HashMap<Key, Track>,
    pending_kills: Vec<(u32, u64, Instant)>,
    gen: u32,
    pub self_pid: u32,
    pub cpu_global: f32,
    pub cores: usize,
    pub mem_used: u64,
    pub mem_total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    pub status: Status,
    pub events: EventLog,
    /// En `bench` no se notifica ni se mata nada.
    pub dry_run: bool,
    started: Instant,
    host: String,
}

impl Monitor {
    pub fn new(cfg: Config, events: EventLog) -> Monitor {
        let refresh = ProcessRefreshKind::nothing()
            .with_cpu()
            .with_memory()
            .with_cmd(UpdateKind::OnlyIfNotSet);
        let sys = System::new().unwrap_or_else(|e| {
            eprintln!("teco: sysinfo no disponible: {e}");
            std::process::exit(1);
        });
        let mut m = Monitor {
            sys,
            refresh,
            cfg,
            cfg_gen: 1,
            needles: Vec::new(),
            cache: HashMap::with_capacity(1024),
            rows: Vec::with_capacity(1024),
            idx: HashMap::with_capacity(1024),
            agg: HashMap::new(),
            root_of: Vec::with_capacity(1024),
            tracks: HashMap::new(),
            pending_kills: Vec::new(),
            gen: 0,
            self_pid: std::process::id(),
            cpu_global: 0.0,
            cores: 0,
            mem_used: 0,
            mem_total: 0,
            swap_used: 0,
            swap_total: 0,
            status: Status::Ok,
            events,
            dry_run: false,
            started: Instant::now(),
            host: System::host_name().unwrap_or_default(),
        };
        m.rebuild_needles();
        m
    }

    fn rebuild_needles(&mut self) {
        self.needles = self.cfg.rules.iter().map(|r| r.needle()).collect();
    }

    pub fn set_config(&mut self, cfg: Config) {
        self.cfg = cfg;
        self.cfg_gen = self.cfg_gen.wrapping_add(1).max(1);
        self.rebuild_needles();
        // Las reglas pudieron cambiar de orden/forma: se reinicia el seguimiento de procesos.
        self.tracks.retain(|k, _| k.rule >= SYS_CPU);
    }

    /// Una muestra completa: refrescar, evaluar reglas, actuar.
    pub fn tick(&mut self) {
        let now = Instant::now();
        self.sample();
        self.check_pending_kills(now);
        self.evaluate(now);
    }

    fn sample(&mut self) {
        self.gen = self.gen.wrapping_add(1);
        let gen = self.gen;
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.sys.refresh_processes_specifics(ProcessesToUpdate::All, true, self.refresh);

        self.cpu_global = self.sys.global_cpu_usage();
        self.cores = self.sys.cpus().len();
        self.mem_used = self.sys.used_memory();
        self.mem_total = self.sys.total_memory();
        self.swap_used = self.sys.used_swap();
        self.swap_total = self.sys.total_swap();

        self.rows.clear();
        self.idx.clear();
        let cfg_gen = self.cfg_gen;
        for (pid, p) in self.sys.processes() {
            // En Linux sysinfo también lista hilos: no son procesos, se ignoran.
            if p.thread_kind().is_some() {
                continue;
            }
            let pid = pid.as_u32();
            let start = p.start_time();
            let meta = self.cache.entry(pid).or_insert_with(|| new_meta(p));
            if meta.start != start {
                *meta = new_meta(p); // PID reutilizado por otro proceso
            }
            if meta.mask_gen != cfg_gen {
                meta.mask = mask_for(&meta.hay, &self.needles);
                meta.mask_gen = cfg_gen;
            }
            meta.seen = gen;
            self.idx.insert(pid, self.rows.len() as u32);
            self.rows.push(Row {
                pid,
                ppid: p.parent().map(|p| p.as_u32()).unwrap_or(0),
                start,
                cpu: p.cpu_usage(),
                mem: p.memory(),
                runtime: p.run_time(),
                name: meta.name.clone(),
                cmd: meta.cmd.clone(),
                mask: meta.mask,
                heat: 0.0,
            });
        }
        self.cache.retain(|_, m| m.seen == gen);
    }

    fn evaluate(&mut self, now: Instant) {
        let gen = self.gen;
        let mut fires: Vec<(Key, bool, Vec<u32>)> = Vec::new();
        let mut resolved: Vec<Key> = Vec::new();

        // --- Sistema ---
        let s = &self.cfg.system;
        let (sfor, scool) = (s.for_secs, s.cooldown_min);
        let mut sys_cands: [Option<Cand>; 3] = [None, None, None];
        if let Some(l) = s.max_mem_percent {
            let v = if self.mem_total > 0 { self.mem_used as f64 * 100.0 / self.mem_total as f64 } else { 0.0 };
            sys_cands[0] = Some(sys_cand(SYS_MEM, "RAM del equipo", Metric::SysMem, v, l, self.mem_used));
        }
        if let Some(l) = s.max_swap_gb {
            let v = self.swap_used as f64 / GB;
            sys_cands[1] = Some(sys_cand(SYS_SWAP, "Swap", Metric::SysSwap, v, l, self.swap_used));
        }
        if let Some(l) = s.max_cpu_percent {
            sys_cands[2] = Some(sys_cand(SYS_CPU, "CPU global", Metric::SysCpu, self.cpu_global as f64, l, 0));
        }
        for c in sys_cands.into_iter().flatten() {
            let (key, name) = (c.key, c.name.clone());
            match step(&mut self.tracks, c, &name, sfor, scool, gen, now) {
                Step::Fire { refire } => fires.push((key, refire, Vec::new())),
                Step::Resolved => resolved.push(key),
                Step::Nothing => {}
            }
        }

        // --- Reglas por proceso ---
        for (ri, rule) in self.cfg.rules.iter().enumerate() {
            let bit = 1u64 << ri;
            let rule_name: Arc<str> = Arc::from(rule.name.as_str());
            if !rule.include_children {
                for row in self.rows.iter_mut() {
                    if row.mask & bit == 0 {
                        continue;
                    }
                    let Some(c) = cand(rule, ri, row.pid, row.start, &row.name, row.mem, row.cpu, row.runtime, 1)
                    else {
                        continue;
                    };
                    row.heat = row.heat.max(c.ratio as f32);
                    if c.ratio < WATCH_RATIO && !self.tracks.contains_key(&c.key) {
                        continue;
                    }
                    let key = c.key;
                    match step(&mut self.tracks, c, &rule_name, rule.for_secs, rule.cooldown_min, gen, now) {
                        Step::Fire { refire } => fires.push((key, refire, vec![row.pid])),
                        Step::Resolved => resolved.push(key),
                        Step::Nothing => {}
                    }
                }
                continue;
            }

            // include_children: cada proceso cuenta dentro de su ancestro coincidente más alto.
            self.root_of.clear();
            self.agg.clear();
            for i in 0..self.rows.len() {
                let mut cur = i;
                let mut root = u32::MAX;
                for _ in 0..MAX_TREE_DEPTH {
                    let r = &self.rows[cur];
                    if r.mask & bit != 0 {
                        root = cur as u32;
                    }
                    if r.ppid == 0 || r.ppid == r.pid {
                        break;
                    }
                    match self.idx.get(&r.ppid) {
                        Some(&j) => cur = j as usize,
                        None => break,
                    }
                }
                self.root_of.push(root);
                if root != u32::MAX {
                    let e = self.agg.entry(root).or_insert((0, 0.0, 0));
                    e.0 += self.rows[i].mem;
                    e.1 += self.rows[i].cpu;
                    e.2 += 1;
                }
            }
            for (&root, &(mem, cpu, members)) in self.agg.iter() {
                let r = &self.rows[root as usize];
                let root_pid = r.pid;
                let Some(c) = cand(rule, ri, r.pid, r.start, &r.name, mem, cpu, r.runtime, members) else {
                    continue;
                };
                let ratio = c.ratio as f32;
                if ratio > 0.0 {
                    for (i, &ro) in self.root_of.iter().enumerate() {
                        if ro == root {
                            self.rows[i].heat = self.rows[i].heat.max(ratio);
                        }
                    }
                }
                if c.ratio < WATCH_RATIO && !self.tracks.contains_key(&c.key) {
                    continue;
                }
                let key = c.key;
                match step(&mut self.tracks, c, &rule_name, rule.for_secs, rule.cooldown_min, gen, now) {
                    Step::Fire { refire } => {
                        let mut pids: Vec<u32> = Vec::with_capacity(members as usize);
                        pids.push(root_pid);
                        for (i, &ro) in self.root_of.iter().enumerate() {
                            if ro == root && i != root as usize {
                                pids.push(self.rows[i].pid);
                            }
                        }
                        fires.push((key, refire, pids));
                    }
                    Step::Resolved => resolved.push(key),
                    Step::Nothing => {}
                }
            }
        }

        // Procesos que desaparecieron o que ya no están ni cerca del límite.
        let mut gone: Vec<(Arc<str>, Arc<str>, u32)> = Vec::new();
        self.tracks.retain(|k, t| {
            if t.seen != gen {
                if t.fired {
                    gone.push((t.rule_name.clone(), t.name.clone(), k.pid));
                }
                return false;
            }
            t.fired || t.over_since.is_some() || t.ratio >= WATCH_RATIO
        });
        for (rule, name, pid) in gone {
            self.events.push(Kind::Resolved, &*rule, format!("{name} ({pid}) terminó"), pid);
        }
        for key in resolved {
            if let Some(t) = self.tracks.get(&key) {
                let text = format!("{} ({}) volvió a la normalidad · {}", t.name, key.pid, t.metric.fmt(t.value));
                let title = t.rule_name.to_string();
                self.events.push(Kind::Resolved, title, text, key.pid);
            }
        }
        for (key, refire, pids) in fires {
            self.fire(key, refire, &pids, now);
        }

        self.status = if self.tracks.values().any(|t| t.fired) {
            Status::Alert
        } else if self.tracks.values().any(|t| t.ratio >= WATCH_RATIO) {
            Status::Watch
        } else {
            Status::Ok
        };
    }

    fn fire(&mut self, key: Key, refire: bool, pids: &[u32], now: Instant) {
        let Some(t) = self.tracks.get(&key) else { return };
        let since = t.over_since.map(|s| now.duration_since(s).as_secs()).unwrap_or(0);
        let is_sys = key.rule >= SYS_CPU;
        let title = format!("🦉 Teco · {}", t.rule_name);
        let mut body = if is_sys {
            format!("{} (límite {})", t.metric.fmt(t.value), t.metric.fmt_limit(t.limit))
        } else {
            let mut b = format!(
                "{} ({}) · {} (límite {})",
                t.name,
                key.pid,
                t.metric.fmt(t.value),
                t.metric.fmt_limit(t.limit)
            );
            if t.metric != Metric::Mem {
                let _ = write!(b, " · {}", fmt_bytes(t.mem));
            }
            if t.members > 1 {
                let _ = write!(b, " · {} procesos", t.members);
            }
            b
        };
        if since > 0 {
            let _ = write!(body, " · hace {}", fmt_dur(since));
        }
        if refire {
            body.push_str(" · sigue así");
        }
        let action = if is_sys { Action::Alert } else { self.cfg.rules[key.rule as usize].action };
        if action == Action::Kill {
            body.push_str(" · terminando…");
        }
        let rule_name = t.rule_name.to_string();

        // Primero avisar y registrar; luego, si toca, terminar.
        self.events.push(Kind::Alert, rule_name.clone(), body.clone(), key.pid);
        if !self.dry_run {
            if self.cfg.notifications {
                notify::native(&title, &body);
            }
            if let Some(url) = self.cfg.webhook_url.as_deref() {
                let mut j = String::with_capacity(256);
                j.push_str("{\"source\":\"teco\",\"host\":");
                json_str(&mut j, &self.host);
                j.push_str(",\"title\":");
                json_str(&mut j, &title);
                j.push_str(",\"body\":");
                json_str(&mut j, &body);
                j.push_str(",\"rule\":");
                json_str(&mut j, &rule_name);
                j.push_str(",\"pid\":");
                json_num(&mut j, key.pid as f64);
                j.push_str(",\"action\":");
                json_str(&mut j, if action == Action::Kill { "kill" } else { "alert" });
                j.push('}');
                notify::webhook(url, j);
            }
        }
        if action == Action::Kill && !self.dry_run {
            for &pid in pids {
                let start = self.idx.get(&pid).map(|&i| self.rows[i as usize].start).unwrap_or(0);
                match self.kill(pid, start) {
                    Ok(msg) => self.events.push(Kind::Kill, rule_name.clone(), msg, pid),
                    Err(e) => self.events.push(Kind::Error, rule_name.clone(), e, pid),
                }
            }
        }
    }

    /// Por qué no se puede terminar este proceso (o `None` si sí se puede).
    pub fn kill_blocker(&self, pid: u32, name: &str) -> Option<&'static str> {
        if pid == self.self_pid {
            Some("es el propio teco")
        } else if matches!(pid, 0 | 1 | 4) {
            Some("PID del sistema")
        } else if self.cfg.is_protected(name) {
            Some("está en la lista de protegidos")
        } else {
            None
        }
    }

    /// Termina un proceso. Unix: SIGTERM y SIGKILL a los 5 s si sigue vivo.
    /// Windows: `TerminateProcess`. `start` evita matar a otro proceso con el PID reciclado.
    /// Termina un proceso o, con `tree`, también a todos sus descendientes (primero los hijos).
    /// Devuelve un resumen; falla solo si no se pudo terminar ninguno.
    pub fn kill_request(&mut self, pid: u32, start: u64, force: bool, tree: bool) -> Result<String, String> {
        // La raíz se valida primero: PID reciclado, protegidos, el propio teco…
        let first = self.kill_one(pid, start, force)?;
        if !tree {
            return Ok(first);
        }
        // Descendientes según la última muestra.
        let mut family: Vec<(u32, u64)> = Vec::new();
        let mut frontier = vec![pid];
        while let Some(parent) = frontier.pop() {
            for r in self.rows.iter().filter(|r| r.ppid == parent && r.pid != parent) {
                if family.len() < 512 && !family.iter().any(|(p, _)| *p == r.pid) {
                    family.push((r.pid, r.start));
                    frontier.push(r.pid);
                }
            }
        }
        let (mut ok, mut fail) = (1usize, 0usize);
        for (p, st) in family.into_iter().rev() {
            match self.kill_one(p, st, force) {
                Ok(_) => ok += 1,
                Err(_) => fail += 1,
            }
        }
        let extra = if fail > 0 { format!(" ({fail} no se pudieron)") } else { String::new() };
        Ok(format!("{first} · árbol: {ok} procesos{extra}"))
    }

    /// Termina un proceso. Unix: SIGTERM y SIGKILL a los 5 s si sigue vivo (o SIGKILL directo
    /// con `force`). Windows: `TerminateProcess`. `start` evita matar a otro proceso con el PID reciclado.
    pub fn kill(&mut self, pid: u32, start: u64) -> Result<String, String> {
        self.kill_one(pid, start, false)
    }

    fn kill_one(&mut self, pid: u32, start: u64, force: bool) -> Result<String, String> {
        let Some(p) = self.sys.process(Pid::from_u32(pid)) else {
            return Err(format!("{pid} ya no existe"));
        };
        let name = p.name().to_string_lossy().into_owned();
        if p.start_time() != start {
            return Err(format!("{name} ({pid}): el PID ahora es de otro proceso"));
        }
        if let Some(why) = self.kill_blocker(pid, &name) {
            return Err(format!("no termino {name} ({pid}): {why}"));
        }
        #[cfg(unix)]
        {
            if force {
                return if p.kill() {
                    Ok(format!("SIGKILL a {name} ({pid})"))
                } else {
                    Err(format!("no pude matar {name} ({pid}) (¿permisos?)"))
                };
            }
            match p.kill_with(sysinfo::Signal::Term) {
                Some(true) => {
                    self.pending_kills.push((pid, start, Instant::now() + KILL_GRACE));
                    Ok(format!("SIGTERM a {name} ({pid})"))
                }
                _ => {
                    if p.kill() {
                        Ok(format!("SIGKILL a {name} ({pid})"))
                    } else {
                        Err(format!("no pude terminar {name} ({pid}) (¿permisos?)"))
                    }
                }
            }
        }
        #[cfg(not(unix))]
        {
            let _ = (KILL_GRACE, force);
            if p.kill() {
                Ok(format!("TerminateProcess a {name} ({pid})"))
            } else {
                Err(format!("no pude terminar {name} ({pid}) (¿permisos?)"))
            }
        }
    }

    fn check_pending_kills(&mut self, now: Instant) {
        if self.pending_kills.is_empty() {
            return;
        }
        let mut i = 0;
        while i < self.pending_kills.len() {
            let (pid, start, deadline) = self.pending_kills[i];
            if now < deadline {
                i += 1;
                continue;
            }
            self.pending_kills.swap_remove(i);
            if let Some(p) = self.sys.process(Pid::from_u32(pid)) {
                if p.start_time() == start {
                    let name = p.name().to_string_lossy().into_owned();
                    let ok = p.kill();
                    let text = if ok {
                        format!("SIGKILL a {name} ({pid}): no respondió a SIGTERM en 5 s")
                    } else {
                        format!("no pude mandar SIGKILL a {name} ({pid})")
                    };
                    self.events.push(if ok { Kind::Kill } else { Kind::Error }, "Terminar", text, pid);
                }
            }
        }
    }

    pub fn uptime_secs(&self) -> u64 {
        self.started.elapsed().as_secs()
    }

    pub fn self_row(&self) -> Option<&Row> {
        self.idx.get(&self.self_pid).map(|&i| &self.rows[i as usize])
    }

    /// Snapshot JSON para el panel. Solo se llama cuando hay una petición.
    /// App dueña de un proceso: la suya, o la del ancestro más cercano que tenga una
    /// (p. ej. un `node` lanzado desde la terminal de VS Code). `bool` = heredada.
    fn app_of(&self, r: &Row) -> Option<(&Arc<str>, bool)> {
        if let Some(a) = self.cache.get(&r.pid).and_then(|m| m.app.as_ref()) {
            return Some((a, false));
        }
        let mut pid = r.ppid;
        for _ in 0..MAX_TREE_DEPTH {
            if pid <= 1 || pid == r.pid {
                break;
            }
            if let Some(a) = self.cache.get(&pid).and_then(|m| m.app.as_ref()) {
                return Some((a, true));
            }
            pid = match self.idx.get(&pid) {
                Some(&i) => self.rows[i as usize].ppid,
                None => break,
            };
        }
        None
    }

    /// Resumen para la vista de dispositivos: solo estado y métricas del sistema,
    /// sin procesos ni eventos (es lo que otros nodos piden cada pocos segundos).
    pub fn write_summary(&self, out: &mut String) {
        out.clear();
        self.write_common(out);
        let fired = self.tracks.values().filter(|t| t.fired).count();
        out.push_str(",\"alerts_fired\":");
        json_num(out, fired as f64);
        out.push_str(",\"alerts_near\":");
        json_num(out, (self.tracks.len() - fired) as f64);
        out.push_str(",\"os\":");
        json_str(out, std::env::consts::OS);
        // Procesos que se están pasando (o están cerca) de una regla: para la botonera.
        let mut list: Vec<(&Key, &Track)> = self.tracks.iter().filter(|(k, _)| k.rule < SYS_CPU).collect();
        list.sort_by(|a, b| {
            b.1.fired.cmp(&a.1.fired).then(b.1.ratio.partial_cmp(&a.1.ratio).unwrap_or(std::cmp::Ordering::Equal))
        });
        out.push_str(",\"hot\":[");
        for (i, (k, t)) in list.iter().take(8).enumerate() {
            if i > 0 {
                out.push(',');
            }
            let state = if t.fired {
                "fired"
            } else if t.over_since.is_some() {
                "pending"
            } else {
                "near"
            };
            out.push_str("{\"pid\":");
            json_num(out, k.pid as f64);
            out.push_str(",\"start\":");
            json_num(out, k.start as f64);
            out.push_str(",\"name\":");
            json_str(out, &t.name);
            out.push_str(",\"rule\":");
            json_str(out, &t.rule_name);
            out.push_str(",\"state\":");
            json_str(out, state);
            out.push_str(",\"value_txt\":");
            json_str(out, &t.metric.fmt(t.value));
            out.push_str(",\"limit_txt\":");
            json_str(out, &t.metric.fmt_limit(t.limit));
            out.push_str(",\"members\":");
            json_num(out, t.members as f64);
            if let Some(why) = self.kill_blocker(k.pid, &t.name) {
                out.push_str(",\"locked\":");
                json_str(out, why);
            }
            out.push('}');
        }
        out.push(']');
        out.push('}');
    }

    pub fn write_state(&self, out: &mut String, sort: SortBy, filter: &str, limit: usize) {
        out.clear();
        self.write_common(out);
        self.write_rest(out, sort, filter, limit);
    }

    /// `{"version":…` hasta `"sys":{…}` (sin cerrar el objeto).
    fn write_common(&self, out: &mut String) {
        out.push_str("{\"version\":");
        json_str(out, env!("CARGO_PKG_VERSION"));
        out.push_str(",\"host\":");
        json_str(out, &self.host);
        out.push_str(",\"now\":");
        json_num(out, now_ms() as f64);
        out.push_str(",\"interval\":");
        json_num(out, self.cfg.interval_secs as f64);
        out.push_str(",\"status\":");
        json_str(out, self.status.as_str());
        out.push_str(",\"allow_kill\":");
        out.push_str(if self.cfg.allow_kill_from_panel { "true" } else { "false" });
        out.push_str(",\"proc_count\":");
        json_num(out, self.rows.len() as f64);

        let me = self.self_row();
        out.push_str(",\"self\":{\"pid\":");
        json_num(out, self.self_pid as f64);
        out.push_str(",\"mem\":");
        json_num(out, me.map(|r| r.mem).unwrap_or(0) as f64);
        out.push_str(",\"cpu\":");
        json_num(out, me.map(|r| r.cpu).unwrap_or(0.0) as f64);
        out.push_str(",\"uptime\":");
        json_num(out, self.uptime_secs() as f64);
        out.push('}');

        let s = &self.cfg.system;
        out.push_str(",\"sys\":{\"cpu\":");
        json_num(out, self.cpu_global as f64);
        out.push_str(",\"cores\":");
        json_num(out, self.cores as f64);
        out.push_str(",\"mem_used\":");
        json_num(out, self.mem_used as f64);
        out.push_str(",\"mem_total\":");
        json_num(out, self.mem_total as f64);
        out.push_str(",\"swap_used\":");
        json_num(out, self.swap_used as f64);
        out.push_str(",\"swap_total\":");
        json_num(out, self.swap_total as f64);
        out.push_str(",\"lim_cpu\":");
        json_num(out, s.max_cpu_percent.unwrap_or(0.0));
        out.push_str(",\"lim_mem\":");
        json_num(out, s.max_mem_percent.unwrap_or(0.0));
        out.push_str(",\"lim_swap_gb\":");
        json_num(out, s.max_swap_gb.unwrap_or(0.0));
        out.push('}');
    }

    fn write_rest(&self, out: &mut String, sort: SortBy, filter: &str, limit: usize) {
        // Alertas activas y cosas "por pasarse".
        let mut tracks: Vec<(&Key, &Track)> = self.tracks.iter().collect();
        tracks.sort_by(|a, b| {
            b.1.fired.cmp(&a.1.fired).then(b.1.ratio.partial_cmp(&a.1.ratio).unwrap_or(std::cmp::Ordering::Equal))
        });
        out.push_str(",\"alerts\":[");
        let mut first = true;
        for (k, t) in tracks {
            if !first {
                out.push(',');
            }
            first = false;
            let state = if t.fired {
                if t.over_since.is_some() { "fired" } else { "recovering" }
            } else if t.over_since.is_some() {
                "pending"
            } else {
                "near"
            };
            out.push_str("{\"state\":");
            json_str(out, state);
            out.push_str(",\"rule\":");
            json_str(out, &t.rule_name);
            out.push_str(",\"sys\":");
            out.push_str(if k.rule >= SYS_CPU { "true" } else { "false" });
            out.push_str(",\"pid\":");
            json_num(out, k.pid as f64);
            out.push_str(",\"start\":");
            json_num(out, k.start as f64);
            out.push_str(",\"name\":");
            json_str(out, &t.name);
            out.push_str(",\"metric\":");
            json_str(out, t.metric.key());
            out.push_str(",\"value\":");
            json_num(out, t.value);
            out.push_str(",\"limit\":");
            json_num(out, t.limit);
            out.push_str(",\"value_txt\":");
            json_str(out, &t.metric.fmt(t.value));
            out.push_str(",\"limit_txt\":");
            json_str(out, &t.metric.fmt_limit(t.limit));
            out.push_str(",\"ratio\":");
            json_num(out, t.ratio);
            out.push_str(",\"mem\":");
            json_num(out, t.mem as f64);
            out.push_str(",\"members\":");
            json_num(out, t.members as f64);
            out.push_str(",\"since\":");
            json_num(out, t.over_since_ms as f64);
            out.push_str(",\"fired_at\":");
            json_num(out, t.fired_ms as f64);
            out.push('}');
        }
        out.push(']');

        // Top N procesos.
        let needle = filter.trim().to_lowercase();
        let mut sel: Vec<u32> = (0..self.rows.len() as u32)
            .filter(|&i| {
                needle.is_empty() || {
                    let r = &self.rows[i as usize];
                    let pid_match = r.pid.to_string() == needle;
                    pid_match
                        || self.cache.get(&r.pid).map(|m| m.hay.contains(needle.as_str())).unwrap_or(false)
                        || self.app_of(r).is_some_and(|(a, _)| a.to_lowercase().contains(needle.as_str()))
                }
            })
            .collect();
        let key = |i: &u32| -> (f64, f64) {
            let r = &self.rows[*i as usize];
            match sort {
                SortBy::Mem => (r.mem as f64, r.cpu as f64),
                SortBy::Cpu => (r.cpu as f64, r.mem as f64),
            }
        };
        let cmp = |a: &u32, b: &u32| key(b).partial_cmp(&key(a)).unwrap_or(std::cmp::Ordering::Equal);
        if sel.len() > limit && limit > 0 {
            sel.select_nth_unstable_by(limit - 1, cmp);
            sel.truncate(limit);
        }
        sel.sort_unstable_by(cmp);
        out.push_str(",\"matched\":");
        json_num(out, sel.len() as f64);
        out.push_str(",\"procs\":[");
        for (n, &i) in sel.iter().enumerate() {
            let r = &self.rows[i as usize];
            if n > 0 {
                out.push(',');
            }
            out.push_str("{\"pid\":");
            json_num(out, r.pid as f64);
            out.push_str(",\"ppid\":");
            json_num(out, r.ppid as f64);
            out.push_str(",\"start\":");
            json_num(out, r.start as f64);
            out.push_str(",\"name\":");
            json_str(out, &r.name);
            out.push_str(",\"cmd\":");
            json_str(out, &r.cmd);
            if let Some((app, via)) = self.app_of(r) {
                out.push_str(",\"app\":");
                json_str(out, app);
                if via {
                    out.push_str(",\"app_via\":true");
                }
            }
            out.push_str(",\"cpu\":");
            json_num(out, r.cpu as f64);
            out.push_str(",\"mem\":");
            json_num(out, r.mem as f64);
            out.push_str(",\"runtime\":");
            json_num(out, r.runtime as f64);
            out.push_str(",\"heat\":");
            json_num(out, r.heat as f64);
            if let Some(why) = self.kill_blocker(r.pid, &r.name) {
                out.push_str(",\"locked\":");
                json_str(out, why);
            }
            out.push('}');
        }
        out.push_str("],\"events\":");
        self.events.write_json(out, 100);
        out.push('}');
    }
}

fn sys_cand(rule: u16, name: &str, metric: Metric, value: f64, limit: f64, mem: u64) -> Cand {
    Cand {
        key: Key { rule, pid: 0, start: 0 },
        name: Arc::from(name),
        metric,
        value,
        limit,
        ratio: if limit > 0.0 { value / limit } else { 0.0 },
        mem,
        members: 0,
    }
}

/// Evalúa los límites de una regla y devuelve el más comprometido.
#[allow(clippy::too_many_arguments)]
fn cand(
    rule: &crate::config::Rule,
    ri: usize,
    pid: u32,
    start: u64,
    name: &Arc<str>,
    mem: u64,
    cpu: f32,
    runtime: u64,
    members: u32,
) -> Option<Cand> {
    let mut best: Option<(Metric, f64, f64, f64)> = None;
    let mut consider = |m: Metric, v: f64, l: Option<f64>| {
        if let Some(l) = l.filter(|l| *l > 0.0) {
            let ratio = v / l;
            if best.map(|b| ratio > b.3).unwrap_or(true) {
                best = Some((m, v, l, ratio));
            }
        }
    };
    consider(Metric::Mem, mem as f64 / GB, rule.max_mem_gb);
    consider(Metric::Cpu, cpu as f64, rule.max_cpu_percent);
    consider(Metric::Runtime, runtime as f64 / 60.0, rule.max_runtime_min);
    let (metric, value, limit, ratio) = best?;
    Some(Cand {
        key: Key { rule: ri as u16, pid, start },
        name: name.clone(),
        metric,
        value,
        limit,
        ratio,
        mem,
        members,
    })
}

/// Máquina de estados de una (regla, proceso):
/// se dispara si se sostiene `for_secs`; avisa una vez; se re-arma al bajar del límite
/// durante `for_secs`, o vuelve a avisar tras `cooldown_min` si sigue arriba.
fn step(
    tracks: &mut HashMap<Key, Track>,
    c: Cand,
    rule_name: &Arc<str>,
    for_secs: u64,
    cooldown_min: f64,
    gen: u32,
    now: Instant,
) -> Step {
    let over = c.ratio >= 1.0;
    let t = tracks.entry(c.key).or_insert_with(|| Track {
        over_since: None,
        under_since: None,
        over_since_ms: 0,
        fired: false,
        fired_at: None,
        fired_ms: 0,
        seen: gen,
        name: c.name.clone(),
        rule_name: rule_name.clone(),
        metric: c.metric,
        value: 0.0,
        limit: 0.0,
        ratio: 0.0,
        mem: 0,
        members: 0,
    });
    t.seen = gen;
    t.metric = c.metric;
    t.value = c.value;
    t.limit = c.limit;
    t.ratio = c.ratio;
    t.mem = c.mem;
    t.members = c.members;
    let hold = Duration::from_secs(for_secs);
    if over {
        t.under_since = None;
        let since = *t.over_since.get_or_insert_with(|| {
            t.over_since_ms = now_ms();
            now
        });
        if !t.fired {
            if now.duration_since(since) >= hold {
                t.fired = true;
                t.fired_at = Some(now);
                t.fired_ms = now_ms();
                return Step::Fire { refire: false };
            }
        } else {
            let cooldown = Duration::from_secs_f64(cooldown_min.max(0.0) * 60.0);
            if cooldown_min > 0.0 && t.fired_at.map(|f| now.duration_since(f) >= cooldown).unwrap_or(false) {
                t.fired_at = Some(now);
                t.fired_ms = now_ms();
                return Step::Fire { refire: true };
            }
        }
    } else {
        t.over_since = None;
        if t.fired {
            let u = *t.under_since.get_or_insert(now);
            if now.duration_since(u) >= hold {
                t.fired = false;
                t.fired_at = None;
                t.under_since = None;
                return Step::Resolved;
            }
        }
    }
    Step::Nothing
}

fn new_meta(p: &sysinfo::Process) -> Meta {
    let name = p.name().to_string_lossy();
    let mut cmd = String::new();
    for (i, a) in p.cmd().iter().enumerate() {
        if i > 0 {
            cmd.push(' ');
        }
        cmd.push_str(&a.to_string_lossy());
        if cmd.len() > CMD_MAX_CHARS * 4 {
            break;
        }
    }
    let app = p.cmd().first().and_then(|a| app_from_path(&a.to_string_lossy()).map(Arc::from));
    let cmd = truncate(&cmd, CMD_MAX_CHARS);
    let mut hay = String::with_capacity(name.len() + cmd.len() + 1);
    hay.push_str(&name.to_lowercase());
    hay.push('\n');
    hay.push_str(&cmd.to_lowercase());
    Meta {
        start: p.start_time(),
        name: Arc::from(name.as_ref()),
        cmd: Arc::from(cmd),
        app,
        hay: hay.into_boxed_str(),
        mask: 0,
        mask_gen: 0,
        seen: 0,
    }
}

/// Nombre del bundle `.app` más externo en la ruta: el de la app, no el de un helper
/// anidado (`/Applications/Google Chrome.app/.../Google Chrome Helper.app/...` → "Google Chrome").
fn app_from_path(path: &str) -> Option<&str> {
    let end = path.find(".app/").or_else(|| path.strip_suffix(".app").map(str::len))?;
    let start = path[..end].rfind('/').map_or(0, |i| i + 1);
    let name = &path[start..end];
    (!name.is_empty()).then_some(name)
}

fn mask_for(hay: &str, needles: &[Option<String>]) -> u64 {
    let mut m = 0u64;
    for (i, n) in needles.iter().enumerate() {
        if n.as_deref().map(|n| hay.contains(n)).unwrap_or(true) {
            m |= 1 << i;
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn rule(toml: &str) -> crate::config::Rule {
        Config::parse(toml).unwrap().rules.remove(0)
    }

    fn mk(ratio_value: f64) -> Cand {
        Cand {
            key: Key { rule: 0, pid: 10, start: 1 },
            name: Arc::from("bun"),
            metric: Metric::Mem,
            value: ratio_value * 4.0,
            limit: 4.0,
            ratio: ratio_value,
            mem: 0,
            members: 1,
        }
    }

    #[test]
    fn sustained_then_once_then_rearm() {
        let mut tracks = HashMap::new();
        let rn: Arc<str> = Arc::from("r");
        let t0 = Instant::now();
        let at = |s: u64| t0 + Duration::from_secs(s);
        // Pico corto: no dispara.
        assert!(matches!(step(&mut tracks, mk(1.2), &rn, 60, 15.0, 1, at(0)), Step::Nothing));
        assert!(matches!(step(&mut tracks, mk(0.5), &rn, 60, 15.0, 2, at(30)), Step::Nothing));
        // Sostenido 60 s: dispara una vez.
        assert!(matches!(step(&mut tracks, mk(1.2), &rn, 60, 15.0, 3, at(40)), Step::Nothing));
        assert!(matches!(step(&mut tracks, mk(1.2), &rn, 60, 15.0, 4, at(100)), Step::Fire { refire: false }));
        assert!(matches!(step(&mut tracks, mk(1.2), &rn, 60, 15.0, 5, at(200)), Step::Nothing));
        // Cooldown de 15 min con el proceso arriba: vuelve a avisar.
        assert!(matches!(step(&mut tracks, mk(1.2), &rn, 60, 15.0, 6, at(100 + 900)), Step::Fire { refire: true }));
        // Baja del límite: se re-arma tras for_secs.
        assert!(matches!(step(&mut tracks, mk(0.5), &rn, 60, 15.0, 7, at(1100)), Step::Nothing));
        assert!(matches!(step(&mut tracks, mk(0.5), &rn, 60, 15.0, 8, at(1160)), Step::Resolved));
        assert!(!tracks.values().next().unwrap().fired);
    }

    #[test]
    fn zero_for_secs_fires_immediately() {
        let mut tracks = HashMap::new();
        let rn: Arc<str> = Arc::from("r");
        assert!(matches!(step(&mut tracks, mk(1.0), &rn, 0, 15.0, 1, Instant::now()), Step::Fire { .. }));
    }

    #[test]
    fn cand_picks_worst_limit() {
        let r = rule("[[rule]]\nname='x'\nmax_mem_gb=4\nmax_runtime_min=40\n");
        let n: Arc<str> = Arc::from("bun.exe");
        let c = cand(&r, 0, 1, 1, &n, (3.1 * GB) as u64, 0.0, 43 * 60, 1).unwrap();
        assert!(c.metric == Metric::Runtime && (c.value - 43.0).abs() < 1e-9 && c.limit == 40.0);
    }

    #[test]
    fn app_names() {
        let chrome = "/Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Framework.framework/Helpers/Google Chrome Helper (Renderer).app/Contents/MacOS/Google Chrome Helper (Renderer)";
        assert_eq!(app_from_path(chrome), Some("Google Chrome"));
        assert_eq!(app_from_path("/Applications/Claude.app/Contents/MacOS/Claude"), Some("Claude"));
        assert_eq!(app_from_path("/Users/me/Applications/Orca.app"), Some("Orca"));
        assert_eq!(app_from_path("/Users/me/.local/bin/claude"), None);
        assert_eq!(app_from_path("/opt/foo.application/bin/x"), None);
        assert_eq!(app_from_path("C:\\Program Files\\x.exe"), None);
    }

    /// Sin app propia, se hereda la del ancestro más cercano.
    #[test]
    fn app_inherited_from_ancestor() {
        let mut m = Monitor::new(Config::parse("").unwrap(), EventLog::new(None));
        let row = |pid, ppid| Row {
            pid, ppid, start: 1, cpu: 0.0, mem: 0, runtime: 0,
            name: Arc::from("x"), cmd: Arc::from(""), mask: 0, heat: 0.0,
        };
        let meta = |app: Option<&str>| Meta {
            start: 1, name: Arc::from("x"), cmd: Arc::from(""), app: app.map(Arc::from),
            hay: "".into(), mask: 0, mask_gen: 0, seen: 0,
        };
        m.rows = vec![row(1, 0), row(50, 1), row(60, 50), row(70, 60), row(80, 1)];
        for (i, r) in m.rows.iter().enumerate() {
            m.idx.insert(r.pid, i as u32);
        }
        for pid in [1, 60, 70, 80] {
            m.cache.insert(pid, meta(None));
        }
        m.cache.insert(50, meta(Some("Orca")));
        let app = |i: usize| m.app_of(&m.rows[i]).map(|(a, via)| (a.to_string(), via));
        assert_eq!(app(1), Some(("Orca".into(), false)));
        assert_eq!(app(3), Some(("Orca".into(), true)));
        assert_eq!(app(4), None);
    }

    #[test]
    fn masks() {
        let needles = vec![None, Some("vitest".to_string()), Some("node".into())];
        assert_eq!(mask_for("bun.exe\nbun x vitest run", &needles), 0b011);
    }

    /// Árbol sintético: bun (vitest) → 2 workers; un hijo que también coincide no se cuenta doble.
    #[test]
    fn children_grouping() {
        let cfg = Config::parse(
            "[[rule]]\nname='Vitest'\nmatch='vitest'\ninclude_children=true\nmax_mem_gb=4\n",
        )
        .unwrap();
        let mut m = Monitor::new(cfg, EventLog::new(None));
        m.dry_run = true;
        let row = |pid, ppid, mem_gb: f64, mask| Row {
            pid,
            ppid,
            start: 1,
            cpu: 10.0,
            mem: (mem_gb * GB) as u64,
            runtime: 60,
            name: Arc::from("bun"),
            cmd: Arc::from(""),
            mask,
            heat: 0.0,
        };
        m.rows = vec![
            row(1, 0, 0.1, 0),      // init
            row(100, 1, 1.0, 1),    // bun vitest (raíz)
            row(101, 100, 1.5, 1),  // worker que también dice "vitest"
            row(102, 101, 2.0, 0),  // nieto
            row(200, 1, 9.0, 0),    // ajeno
        ];
        for (i, r) in m.rows.iter().enumerate() {
            m.idx.insert(r.pid, i as u32);
        }
        m.gen = 1;
        m.evaluate(Instant::now());
        let t = m.tracks.get(&Key { rule: 0, pid: 100, start: 1 }).expect("raíz agrupada");
        assert_eq!(t.members, 3);
        assert!((t.value - 4.5).abs() < 0.01);
        assert!(t.fired);
        assert!(!m.tracks.keys().any(|k| k.pid == 101));
        assert!(m.status == Status::Alert);
        assert!(m.rows[3].heat >= 1.0 && m.rows[4].heat == 0.0);
    }
}
