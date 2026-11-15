//! `teco.toml`: carga, valores por defecto y recarga en caliente (por mtime).

use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const DEFAULT_TOML: &str = include_str!("../assets/default.toml");

/// Máximo de reglas: cada proceso guarda qué reglas le aplican en un `u64`.
pub const MAX_RULES: usize = 64;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "d_bind")]
    pub bind: String,
    #[serde(default = "d_interval")]
    pub interval_secs: u64,
    #[serde(default)]
    pub allow_kill_from_panel: bool,
    #[serde(default = "d_true")]
    pub notifications: bool,
    #[serde(default)]
    pub webhook_url: Option<String>,
    #[serde(default)]
    pub protected: Vec<String>,
    #[serde(default)]
    pub system: SystemLimits,
    #[serde(default, rename = "rule")]
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemLimits {
    pub max_mem_percent: Option<f64>,
    pub max_swap_gb: Option<f64>,
    pub max_cpu_percent: Option<f64>,
    #[serde(default)]
    pub for_secs: u64,
    #[serde(default = "d_cooldown")]
    pub cooldown_min: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Alert,
    Kill,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub name: String,
    #[serde(rename = "match", default = "d_star")]
    pub pattern: String,
    #[serde(default)]
    pub include_children: bool,
    pub max_mem_gb: Option<f64>,
    pub max_cpu_percent: Option<f64>,
    pub max_runtime_min: Option<f64>,
    #[serde(default)]
    pub for_secs: u64,
    #[serde(default = "d_cooldown")]
    pub cooldown_min: f64,
    #[serde(default = "d_action")]
    pub action: Action,
}

fn d_bind() -> String {
    "127.0.0.1:7777".into()
}
fn d_interval() -> u64 {
    2
}
fn d_true() -> bool {
    true
}
fn d_cooldown() -> f64 {
    15.0
}
fn d_star() -> String {
    "*".into()
}
fn d_action() -> Action {
    Action::Alert
}

impl Rule {
    /// `None` = cualquiera. Si no, el patrón en minúsculas.
    pub fn needle(&self) -> Option<String> {
        let p = self.pattern.trim();
        if p.is_empty() || p == "*" {
            None
        } else {
            Some(p.to_lowercase())
        }
    }
    pub fn has_limits(&self) -> bool {
        self.max_mem_gb.is_some() || self.max_cpu_percent.is_some() || self.max_runtime_min.is_some()
    }
}

impl Config {
    pub fn parse(text: &str) -> Result<Config, String> {
        let mut c: Config = toml::from_str(text).map_err(|e| e.to_string())?;
        c.interval_secs = c.interval_secs.clamp(1, 3600);
        if c.rules.len() > MAX_RULES {
            return Err(format!("máximo {MAX_RULES} reglas (hay {})", c.rules.len()));
        }
        for r in &c.rules {
            if !r.has_limits() {
                return Err(format!(
                    "la regla \"{}\" no tiene límites (max_mem_gb, max_cpu_percent o max_runtime_min)",
                    r.name
                ));
            }
        }
        Ok(c)
    }

    pub fn is_protected(&self, name: &str) -> bool {
        self.protected.iter().any(|p| p.eq_ignore_ascii_case(name))
    }
}

/// Carpeta de configuración del sistema:
/// macOS `~/Library/Application Support/teco`, Windows `%APPDATA%\teco`,
/// otros `$XDG_CONFIG_HOME/teco` o `~/.config/teco`.
pub fn config_dir() -> PathBuf {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    #[cfg(target_os = "windows")]
    let base = env("APPDATA").unwrap_or_else(|| PathBuf::from("."));
    #[cfg(target_os = "macos")]
    let base = env("HOME")
        .map(|h| h.join("Library").join("Application Support"))
        .unwrap_or_else(|| PathBuf::from("."));
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let base = env("XDG_CONFIG_HOME")
        .or_else(|| env("HOME").map(|h| h.join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("teco")
}

pub fn default_path() -> PathBuf {
    config_dir().join("teco.toml")
}

/// Lee la config; si no existe, escribe la de por defecto y la usa.
pub fn load_or_create(path: &Path) -> Result<Config, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Config::parse(&text).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Err(e) = std::fs::write(path, DEFAULT_TOML) {
                eprintln!("teco: no pude crear {}: {e}", path.display());
            } else {
                eprintln!("teco: config creada en {}", path.display());
            }
            Config::parse(DEFAULT_TOML)
        }
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

pub fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_parses() {
        let c = Config::parse(DEFAULT_TOML).unwrap();
        assert_eq!(c.rules.len(), 3);
        assert_eq!(c.rules[1].pattern, "vitest");
        assert!(c.rules[1].include_children);
        assert_eq!(c.rules[1].cooldown_min, 15.0);
        assert_eq!(c.rules[0].action, Action::Alert);
        assert!(c.is_protected("LSASS.EXE"));
        assert_eq!(c.system.max_mem_percent, Some(90.0));
    }

    #[test]
    fn rule_without_limits_is_error() {
        let e = Config::parse("[[rule]]\nname='x'\nmatch='a'\n").unwrap_err();
        assert!(e.contains("no tiene límites"));
    }

    #[test]
    fn unknown_field_is_error() {
        assert!(Config::parse("intervalo = 3").is_err());
    }
}
