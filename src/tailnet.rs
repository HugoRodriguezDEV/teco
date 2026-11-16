//! Descubrimiento de nodos por Tailscale: la vista de Dispositivos se arma sola con
//! las máquinas de la tailnet, sin editar `teco.toml`.
//!
//! Costo: solo se consulta cuando alguien pide `/api/devices` (el panel abierto), y el
//! resultado se guarda 30 s. Con el panel cerrado no se ejecuta nada.

use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const CACHE: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct Peer {
    /// Nombre corto de la máquina en la tailnet (`msi-zotz`).
    pub name: String,
    /// Origen HTTPS del panel (`https://msi-zotz.tail59ed16.ts.net`).
    pub url: String,
    pub online: bool,
    /// Lleva `tag:teco`: se espera que tenga Teco, y si no responde se muestra caído.
    pub teco: bool,
}

#[derive(Deserialize)]
struct Status {
    #[serde(rename = "BackendState", default)]
    backend_state: String,
    #[serde(rename = "Peer", default)]
    peer: HashMap<String, RawPeer>,
}

#[derive(Deserialize)]
struct RawPeer {
    #[serde(rename = "DNSName", default)]
    dns_name: String,
    #[serde(rename = "HostName", default)]
    host_name: String,
    #[serde(rename = "Online", default)]
    online: bool,
    #[serde(rename = "Tags", default)]
    tags: Option<Vec<String>>,
    #[serde(rename = "OS", default)]
    os: String,
}

pub struct Discovery {
    at: Option<Instant>,
    peers: Vec<Peer>,
    pub running: bool,
}

impl Discovery {
    pub const fn new() -> Discovery {
        Discovery { at: None, peers: Vec::new(), running: false }
    }

    /// Nodos de la tailnet (sin esta máquina). Usa la copia de hace <30 s si existe.
    pub fn peers(&mut self) -> &[Peer] {
        if self.at.map(|t| t.elapsed() < CACHE).unwrap_or(false) {
            return &self.peers;
        }
        self.at = Some(Instant::now());
        match status() {
            Some(s) => {
                self.running = s.backend_state == "Running";
                self.peers = parse_peers(s);
            }
            None => {
                self.running = false;
                self.peers.clear();
            }
        }
        &self.peers
    }
}

fn parse_peers(s: Status) -> Vec<Peer> {
    let mut v: Vec<Peer> = s
        .peer
        .into_values()
        // Teléfonos y tablets nunca corren Teco.
        .filter(|p| !matches!(p.os.as_str(), "iOS" | "android"))
        .filter_map(|p| {
            let dns = p.dns_name.trim_end_matches('.').to_ascii_lowercase();
            if dns.is_empty() || !dns.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
                return None;
            }
            let teco = p.tags.as_deref().unwrap_or(&[]).iter().any(|t| t == "tag:teco");
            let name = dns.split('.').next().unwrap_or(&p.host_name).to_string();
            Some(Peer { name, url: format!("https://{dns}"), online: p.online, teco })
        })
        .collect();
    v.sort_by(|a, b| b.teco.cmp(&a.teco).then(a.name.cmp(&b.name)));
    v
}

/// Ruta a la CLI de Tailscale.
pub fn cli() -> Option<PathBuf> {
    let candidates: &[&str] = if cfg!(target_os = "macos") {
        &["/Applications/Tailscale.app/Contents/MacOS/Tailscale", "/usr/local/bin/tailscale", "/opt/homebrew/bin/tailscale"]
    } else if cfg!(windows) {
        &[r"C:\Program Files\Tailscale\tailscale.exe"]
    } else {
        &["/usr/bin/tailscale", "/usr/local/bin/tailscale"]
    };
    candidates.iter().map(PathBuf::from).find(|p| p.exists()).or_else(|| {
        // Último recurso: el PATH.
        let exe = if cfg!(windows) { "tailscale.exe" } else { "tailscale" };
        std::env::var_os("PATH").and_then(|path| std::env::split_paths(&path).map(|d| d.join(exe)).find(|p| p.exists()))
    })
}

fn status() -> Option<Status> {
    let mut cmd = Command::new(cli()?);
    cmd.args(["status", "--json"]).stdin(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        // teco.exe no tiene consola: sin esto Windows abriría una ventana negra un instante.
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().ok()?;
    if !out.status.success() && out.stdout.is_empty() {
        return None;
    }
    serde_json::from_slice(&out.stdout).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tailscale_status() {
        let json = r#"{
          "BackendState": "Running",
          "Self": {"DNSName": "macbook-pro-hugo.tail59ed16.ts.net."},
          "Peer": {
            "nodekey:a": {"DNSName": "msi-zotz.tail59ed16.ts.net.", "HostName": "DESKTOP-MPD93MC",
                          "Online": true, "OS": "windows", "Tags": ["tag:teco", "tag:zotz"]},
            "nodekey:b": {"DNSName": "pixel.tail59ed16.ts.net.", "HostName": "pixel", "Online": true, "OS": "android"},
            "nodekey:c": {"DNSName": "laptop-vieja.tail59ed16.ts.net.", "HostName": "vieja", "Online": false, "OS": "linux"},
            "nodekey:d": {"DNSName": "", "HostName": "raro", "Online": true, "OS": "linux"}
          }
        }"#;
        let s: Status = serde_json::from_str(json).unwrap();
        assert_eq!(s.backend_state, "Running");
        let p = parse_peers(s);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].name, "msi-zotz");
        assert_eq!(p[0].url, "https://msi-zotz.tail59ed16.ts.net");
        assert!(p[0].teco && p[0].online);
        assert_eq!(p[1].name, "laptop-vieja");
        assert!(!p[1].teco && !p[1].online);
    }
}
