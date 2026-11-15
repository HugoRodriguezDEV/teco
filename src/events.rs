//! Registro de eventos: en memoria (últimos 200, para el panel) y en
//! `teco-events.log` rotativo (1 MB × 3 archivos).

use crate::util::{iso_utc, json_num, json_str, now_ms};
use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

const MEM_EVENTS: usize = 200;
const MAX_LOG_BYTES: u64 = 1024 * 1024;
const LOG_FILES: usize = 3; // teco-events.log, .1, .2

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Alert,
    Kill,
    Resolved,
    Info,
    Error,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Alert => "alert",
            Kind::Kill => "kill",
            Kind::Resolved => "resolved",
            Kind::Info => "info",
            Kind::Error => "error",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Event {
    pub ts_ms: u64,
    pub kind: Kind,
    pub title: String,
    pub text: String,
    pub pid: u32,
}

pub struct EventLog {
    mem: VecDeque<Event>,
    file: Option<PathBuf>,
}

impl EventLog {
    pub fn new(file: Option<PathBuf>) -> Self {
        EventLog { mem: VecDeque::with_capacity(MEM_EVENTS), file }
    }

    pub fn push(&mut self, kind: Kind, title: impl Into<String>, text: impl Into<String>, pid: u32) {
        let ev = Event { ts_ms: now_ms(), kind, title: title.into(), text: text.into(), pid };
        if let Some(path) = &self.file {
            if let Err(e) = append(path, &ev) {
                eprintln!("teco: no pude escribir {}: {e}", path.display());
            }
        }
        if self.mem.len() == MEM_EVENTS {
            self.mem.pop_front();
        }
        self.mem.push_back(ev);
    }

    /// Los más recientes primero.
    pub fn write_json(&self, out: &mut String, limit: usize) {
        out.push('[');
        for (i, e) in self.mem.iter().rev().take(limit).enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str("{\"ts\":");
            json_num(out, e.ts_ms as f64);
            out.push_str(",\"kind\":");
            json_str(out, e.kind.as_str());
            out.push_str(",\"title\":");
            json_str(out, &e.title);
            out.push_str(",\"text\":");
            json_str(out, &e.text);
            out.push_str(",\"pid\":");
            json_num(out, e.pid as f64);
            out.push('}');
        }
        out.push(']');
    }
}

fn rotated(path: &std::path::Path, n: usize) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(format!(".{n}"));
    PathBuf::from(s)
}

fn append(path: &std::path::Path, ev: &Event) -> std::io::Result<()> {
    if std::fs::metadata(path).map(|m| m.len() >= MAX_LOG_BYTES).unwrap_or(false) {
        for n in (1..LOG_FILES).rev() {
            let from = if n == 1 { path.to_path_buf() } else { rotated(path, n - 1) };
            let _ = std::fs::rename(&from, rotated(path, n));
        }
    }
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    let line = format!(
        "{} [{}] {} — {}\n",
        iso_utc(ev.ts_ms),
        ev.kind.as_str(),
        ev.title,
        ev.text.replace('\n', " ")
    );
    f.write_all(line.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_and_rotation() {
        let dir = std::env::temp_dir().join(format!("teco-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("teco-events.log");
        let mut log = EventLog::new(Some(path.clone()));
        let big = "x".repeat(4096);
        for i in 0..(MEM_EVENTS + 600) {
            log.push(Kind::Info, format!("e{i}"), big.as_str(), 0);
        }
        assert_eq!(log.mem.len(), MEM_EVENTS);
        assert!(path.exists() && rotated(&path, 1).exists() && rotated(&path, 2).exists());
        assert!(!rotated(&path, 3).exists());
        for p in [path.clone(), rotated(&path, 1), rotated(&path, 2)] {
            assert!(std::fs::metadata(p).unwrap().len() <= MAX_LOG_BYTES + 8192);
        }
        let mut s = String::new();
        log.write_json(&mut s, 1);
        assert!(s.contains(&format!("\"e{}\"", MEM_EVENTS + 599)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
