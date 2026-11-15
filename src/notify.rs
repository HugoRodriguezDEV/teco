//! Avisos hacia afuera: notificación nativa y, con la feature `webhook`, POST JSON.
//! Todo se manda en un hilo corto para no frenar el muestreo (son eventos raros).

pub fn native(title: &str, body: &str) {
    let (title, body) = (title.to_owned(), body.to_owned());
    let spawned = std::thread::Builder::new()
        .name("teco-notify".into())
        .stack_size(256 * 1024)
        .spawn(move || show(&title, &body));
    if let Err(e) = spawned {
        eprintln!("teco: no pude lanzar la notificación: {e}");
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn show(title: &str, body: &str) {
    let r = notify_rust::Notification::new().summary(title).body(body).show();
    if let Err(e) = r {
        eprintln!("teco: notificación falló: {e}");
    }
}

/// En Linux (solo desarrollo) usamos `notify-send` si existe: cero dependencias.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn show(title: &str, body: &str) {
    let r = std::process::Command::new("notify-send")
        .args(["--app-name=Teco", title, body])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    if r.is_err() {
        eprintln!("teco: {title} — {body}");
    }
}

#[cfg(feature = "webhook")]
pub fn webhook(url: &str, json: String) {
    let url = url.to_owned();
    let _ = std::thread::Builder::new().name("teco-webhook".into()).spawn(move || {
        let r = ureq::post(&url)
            .header("Content-Type", "application/json")
            .header("User-Agent", concat!("teco/", env!("CARGO_PKG_VERSION")))
            .send(json.as_bytes());
        if let Err(e) = r {
            eprintln!("teco: webhook falló: {e}");
        }
    });
}

#[cfg(not(feature = "webhook"))]
pub fn webhook(_url: &str, _json: String) {}
