//! Panel web. Servidor HTTP/1.1 síncrono, un solo hilo, sin dependencias:
//! duerme en `accept()` y no cuesta nada mientras nadie mira. El JSON se arma
//! solo cuando llega una petición.

use crate::events::Kind;
use crate::monitor::{Monitor, SortBy};
use crate::util::json_str;
use std::io::{Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

static DISCOVERY: Mutex<crate::tailnet::Discovery> = Mutex::new(crate::tailnet::Discovery::new());

const INDEX_HTML: &str = include_str!("../assets/index.html");
const DEVICES_HTML: &str = include_str!("../assets/devices.html");
const LOGO_SVG: &str = include_str!("../assets/teco.svg");
const MAX_HEAD: usize = 8 * 1024;
const MAX_BODY: usize = 1024;

pub fn bind(addr: &str) -> std::io::Result<TcpListener> {
    TcpListener::bind(addr)
}

/// Por qué listener llegó la petición. Se decide por el puerto, nunca por la cabecera
/// `Host` (que la escribe el cliente): `Tailnet` es el destino de `tailscale serve`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Via {
    Local,
    Tailnet,
}

pub fn spawn(listener: TcpListener, mon: Arc<Mutex<Monitor>>, via: Via) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name(if via == Via::Local { "teco-http".into() } else { "teco-http-tailnet".into() })
        .stack_size(256 * 1024)
        .spawn(move || {
            let mut buf = Vec::with_capacity(MAX_HEAD);
            let mut json = String::new();
            for conn in listener.incoming() {
                let Ok(stream) = conn else { continue };
                if let Err(e) = handle(stream, &mon, &mut buf, &mut json, via) {
                    if e.kind() != std::io::ErrorKind::WouldBlock && e.kind() != std::io::ErrorKind::TimedOut {
                        eprintln!("teco: http: {e}");
                    }
                }
                // No retener memoria si alguien pidió algo enorme.
                if json.capacity() > 512 * 1024 {
                    json = String::new();
                }
            }
        })?;
    Ok(())
}

struct Req<'a> {
    method: &'a str,
    path: &'a str,
    query: &'a str,
    host: Option<&'a str>,
    origin: Option<&'a str>,
    teco_header: bool,
    body: &'a [u8],
    via: Via,
}

fn handle(mut s: TcpStream, mon: &Mutex<Monitor>, buf: &mut Vec<u8>, json: &mut String, via: Via) -> std::io::Result<()> {
    s.set_read_timeout(Some(Duration::from_secs(3)))?;
    s.set_write_timeout(Some(Duration::from_secs(3)))?;
    let local_ip = s.local_addr().map(|a| a.ip()).ok();
    buf.clear();
    let mut chunk = [0u8; 2048];
    let head_end = loop {
        let n = s.read(&mut chunk)?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(p) = find(buf, b"\r\n\r\n") {
            break p + 4;
        }
        if buf.len() > MAX_HEAD {
            return respond(&mut s, false, 431, "text/plain", b"cabeceras demasiado grandes", false);
        }
    };
    let head = std::str::from_utf8(&buf[..head_end]).unwrap_or("");
    let mut lines = head.split("\r\n");
    let mut first = lines.next().unwrap_or("").split(' ');
    let method = first.next().unwrap_or("");
    let target = first.next().unwrap_or("/");
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let (mut host, mut origin, mut teco_header, mut clen) = (None, None, false, 0usize);
    for l in lines {
        let Some((k, v)) = l.split_once(':') else { continue };
        let v = v.trim();
        if k.eq_ignore_ascii_case("host") {
            host = Some(v);
        } else if k.eq_ignore_ascii_case("origin") {
            origin = Some(v);
        } else if k.eq_ignore_ascii_case("x-teco") {
            teco_header = true;
        } else if k.eq_ignore_ascii_case("content-length") {
            clen = v.parse().unwrap_or(usize::MAX);
        }
    }
    if clen > MAX_BODY {
        return respond(&mut s, false, 413, "text/plain", b"cuerpo demasiado grande", false);
    }
    let have = buf.len() - head_end;
    if have < clen {
        let mut rest = vec![0u8; clen - have];
        s.read_exact(&mut rest)?;
        // `head` toma prestado `buf`; se reconstruye la petición con el cuerpo completo.
        let mut full = buf[head_end..].to_vec();
        full.extend_from_slice(&rest);
        let req = Req { method, path, query, host, origin, teco_header, body: &full, via };
        return route(&mut s, &req, mon, json, local_ip);
    }
    let body = &buf[head_end..head_end + clen];
    let req = Req { method, path, query, host, origin, teco_header, body, via };
    route(&mut s, &req, mon, json, local_ip)
}

fn route(
    s: &mut TcpStream,
    req: &Req,
    mon: &Mutex<Monitor>,
    json: &mut String,
    local_ip: Option<IpAddr>,
) -> std::io::Result<()> {
    if !host_ok(req.host, local_ip) {
        return respond(s, false, 403, "text/plain", b"host no permitido", false);
    }
    // HEAD = GET sin cuerpo.
    let method = if req.method == "HEAD" { "GET" } else { req.method };
    let head_only = req.method == "HEAD";
    match (method, req.path) {
        ("GET", "/") => {
            // La vista de dispositivos pide /api/summary a cada nodo: se permiten esos orígenes.
            let mut connect: String = lock(mon).cfg.devices.iter().map(|d| format!(" {}", d.url)).collect();
            if lock(mon).cfg.discover_tailnet {
                // Nodos descubiertos en la tailnet (y los que aparezcan después sin recargar).
                connect.push_str(" https://*.ts.net");
            }
            let extra = format!("Content-Security-Policy: {}\r\n", csp(&connect));
            respond_with(s, head_only, 200, "text/html; charset=utf-8", DEVICES_HTML.as_bytes(), false, &extra)
        }
        ("GET", "/equipo") | ("GET", "/index.html") => {
            respond(s, head_only, 200, "text/html; charset=utf-8", INDEX_HTML.as_bytes(), false)
        }
        ("GET", "/api/devices") => {
            let (manual, discover) = {
                let m = lock(mon);
                (m.cfg.devices.clone(), m.cfg.discover_tailnet)
            };
            json.clear();
            json.push('[');
            let mut first = true;
            let mut item = |json: &mut String, name: &str, url: &str, extra: &str| {
                if !first {
                    json.push(',');
                }
                first = false;
                json.push_str("{\"name\":");
                json_str(json, name);
                json.push_str(",\"url\":");
                json_str(json, url);
                json.push_str(extra);
                json.push('}');
            };
            for d in &manual {
                item(json, &d.name, &d.url, "");
            }
            if discover {
                // Fuera del candado del monitor: `tailscale status` tarda unos ms.
                let mut disc = DISCOVERY.lock().unwrap_or_else(|e| e.into_inner());
                for p in disc.peers() {
                    if manual.iter().any(|d| d.url == p.url) {
                        continue;
                    }
                    let extra = format!(
                        ",\"auto\":true,\"teco\":{},\"online\":{}",
                        p.teco, p.online
                    );
                    item(json, &p.name, &p.url, &extra);
                }
            }
            json.push(']');
            respond(s, head_only, 200, "application/json", json.as_bytes(), false)
        }
        ("GET", "/api/summary") => {
            lock(mon).write_summary(json);
            // Lectura entre nodos: CORS solo para orígenes de la tailnet o de esta máquina.
            let extra = match req.origin.filter(|o| cors_ok(o)) {
                Some(o) => format!("Access-Control-Allow-Origin: {o}\r\nVary: Origin\r\n"),
                None => "Vary: Origin\r\n".to_string(),
            };
            respond_with(s, head_only, 200, "application/json", json.as_bytes(), false, &extra)
        }
        ("GET", "/logo.svg") | ("GET", "/favicon.svg") | ("GET", "/favicon.ico") => {
            respond(s, head_only, 200, "image/svg+xml", LOGO_SVG.as_bytes(), true)
        }
        ("GET", "/api/state") => {
            let sort = if param(req.query, "sort") == Some("cpu") { SortBy::Cpu } else { SortBy::Mem };
            let filter = param(req.query, "q").map(url_decode).unwrap_or_default();
            let n = param(req.query, "n").and_then(|v| v.parse().ok()).unwrap_or(25usize).clamp(1, 200);
            {
                let m = lock(mon);
                m.write_state(json, sort, &filter, n);
            }
            respond(s, head_only, 200, "application/json", json.as_bytes(), false)
        }
        ("POST", "/api/kill") => {
            // Anti-CSRF: exige cabecera propia (fuerza preflight, que no respondemos)
            // y, si hay Origin, que coincida con Host.
            let same_origin = match (req.origin, req.host) {
                (Some(o), Some(h)) => o.split("://").nth(1) == Some(h),
                (None, _) => true,
                _ => false,
            };
            if !req.teco_header || !same_origin {
                return respond(s, head_only, 403, "text/plain", b"origen no permitido", false);
            }
            let body = std::str::from_utf8(req.body).unwrap_or("");
            let pid: Option<u32> = param(body, "pid").and_then(|v| v.parse().ok());
            let start: Option<u64> = param(body, "start").and_then(|v| v.parse().ok());
            let (Some(pid), Some(start)) = (pid, start) else {
                return respond(s, head_only, 400, "text/plain", b"faltan pid y start", false);
            };
            let mut m = lock(mon);
            if !m.cfg.allow_kill_from_panel {
                drop(m);
                return respond(s, head_only, 403, "text/plain", b"allow_kill_from_panel = false en teco.toml", false);
            }
            let r = m.kill(pid, start);
            let (code, msg) = match r {
                Ok(msg) => {
                    m.events.push(Kind::Kill, panel_title(req.via), msg.clone(), pid);
                    (200, msg)
                }
                Err(e) => {
                    m.events.push(Kind::Error, panel_title(req.via), e.clone(), pid);
                    (409, e)
                }
            };
            drop(m);
            respond(s, head_only, code, "text/plain; charset=utf-8", msg.as_bytes(), false)
        }
        ("GET", _) => respond(s, head_only, 404, "text/plain", b"no existe", false),
        _ => respond(s, head_only, 405, "text/plain", b"metodo no permitido", false),
    }
}

fn panel_title(via: Via) -> &'static str {
    match via {
        Via::Local => "Desde el panel",
        Via::Tailnet => "Desde el panel (tailnet)",
    }
}

fn lock(m: &Mutex<Monitor>) -> std::sync::MutexGuard<'_, Monitor> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Protección contra DNS rebinding: el Host debe ser loopback, una IP literal
/// (los ataques de rebinding usan nombres), la IP local, o un nombre MagicDNS de Tailscale.
fn host_ok(host: Option<&str>, local_ip: Option<IpAddr>) -> bool {
    let Some(h) = host else { return true }; // HTTP/1.0 sin Host
    let name = if let Some(rest) = h.strip_prefix('[') {
        rest.split(']').next().unwrap_or("")
    } else {
        h.rsplit_once(':').map(|(n, _)| n).unwrap_or(h)
    };
    let name = name.trim_end_matches('.').to_ascii_lowercase();
    if name == "localhost" || name.ends_with(".localhost") || name.ends_with(".ts.net") {
        return true;
    }
    match name.parse::<IpAddr>() {
        Ok(ip) => ip.is_loopback() || Some(ip) == local_ip || !ip.is_unspecified(),
        Err(_) => false,
    }
}

/// Orígenes que pueden leer `/api/summary` desde otro nodo: loopback, `localhost`,
/// nombres `*.ts.net` e IPs de Tailscale (100.64.0.0/10 y fd7a:115c:a1e0::/48).
fn cors_ok(origin: &str) -> bool {
    let Some(rest) = origin.strip_prefix("https://").or_else(|| origin.strip_prefix("http://")) else {
        return false;
    };
    let name = if let Some(r) = rest.strip_prefix('[') {
        r.split(']').next().unwrap_or("")
    } else {
        rest.rsplit_once(':').map(|(n, _)| n).unwrap_or(rest)
    };
    let name = name.trim_end_matches('.').to_ascii_lowercase();
    if name == "localhost" || name.ends_with(".localhost") || name.ends_with(".ts.net") {
        return true;
    }
    match name.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => ip.is_loopback() || (ip.octets()[0] == 100 && ip.octets()[1] & 0xC0 == 64),
        Ok(IpAddr::V6(ip)) => ip.is_loopback() || ip.segments()[..3] == [0xfd7a, 0x115c, 0xa1e0],
        Err(_) => false,
    }
}

fn csp(connect: &str) -> String {
    format!(
        "default-src 'self'; connect-src 'self'{connect}; style-src 'unsafe-inline'; script-src 'unsafe-inline'; img-src 'self' data:"
    )
}

fn respond(s: &mut TcpStream, head_only: bool, code: u16, ctype: &str, body: &[u8], cache: bool) -> std::io::Result<()> {
    respond_with(s, head_only, code, ctype, body, cache, "")
}

/// `extra`: cabeceras adicionales terminadas en `\r\n`. Si trae su propia CSP, reemplaza la de por defecto.
fn respond_with(
    s: &mut TcpStream,
    head_only: bool,
    code: u16,
    ctype: &str,
    body: &[u8],
    cache: bool,
    extra: &str,
) -> std::io::Result<()> {
    let reason = match code {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Payload Too Large",
        _ => "Request Header Fields Too Large",
    };
    let default_csp;
    let csp_line = if extra.contains("Content-Security-Policy:") {
        ""
    } else {
        default_csp = format!("Content-Security-Policy: {}\r\n", csp(""));
        &default_csp
    };
    let head = format!(
        "HTTP/1.1 {code} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\n\
         Cache-Control: {}\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\n\
         {csp_line}{extra}Referrer-Policy: no-referrer\r\nConnection: close\r\n\r\n",
        body.len(),
        if cache { "max-age=86400" } else { "no-store" }
    );
    s.write_all(head.as_bytes())?;
    if !head_only {
        s.write_all(body)?;
    }
    s.flush()
}

fn find(h: &[u8], n: &[u8]) -> Option<usize> {
    h.windows(n.len()).position(|w| w == n)
}

fn param<'a>(q: &'a str, key: &str) -> Option<&'a str> {
    q.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        (k == key).then_some(v)
    })
}

fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                let hex = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok());
                match hex {
                    Some(v) => {
                        out.push(v);
                        i += 2;
                    }
                    None => out.push(b'%'),
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts() {
        let lo: Option<IpAddr> = "127.0.0.1".parse().ok();
        assert!(host_ok(Some("127.0.0.1:7777"), lo));
        assert!(host_ok(Some("localhost:7777"), lo));
        assert!(host_ok(Some("[::1]:7777"), lo));
        assert!(host_ok(Some("100.64.1.2:7777"), lo));
        assert!(host_ok(Some("zotz.tail1234.ts.net:7777"), lo));
        assert!(!host_ok(Some("evil.example.com:7777"), lo));
    }

    #[test]
    fn cors_origins() {
        for ok in ["http://127.0.0.1:7777", "http://localhost:7777", "https://msi.tail1.ts.net", "http://100.101.2.3:7777", "http://[fd7a:115c:a1e0::1]:7777"] {
            assert!(cors_ok(ok), "{ok}");
        }
        for bad in ["https://evil.example.com", "http://8.8.8.8", "http://100.128.0.1", "null", "127.0.0.1"] {
            assert!(!cors_ok(bad), "{bad}");
        }
    }

    #[test]
    fn decode() {
        assert_eq!(url_decode("bun%20x+vitest%2"), "bun x vitest%2");
        assert_eq!(url_decode("%C3%B1"), "ñ");
        assert_eq!(param("sort=cpu&q=a", "q"), Some("a"));
    }
}
