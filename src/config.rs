//! Files on disk: application settings, saved profiles and the last state
//! sent to the controller.
//!
//! Layout under `dirs::config_dir()/bigbigctl/` (override with
//! `BIGBIGCTL_CONFIG_DIR`, useful for tests):
//!
//! * `settings.toml` — app preferences (language, tray, autostart…);
//! * `state.toml`    — last configuration applied, per controller UUID;
//! * `profiles/*.toml` — named profiles, applicable with one click.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::i18n::Lang;
use crate::protocol::profile::Profile;
use crate::{Error, Result};

pub const APP_DIR: &str = "bigbigctl";

/// Root configuration directory.
pub fn config_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("BIGBIGCTL_CONFIG_DIR") {
        return PathBuf::from(p);
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_DIR)
}

fn read_toml<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> Result<T> {
    match std::fs::read_to_string(path) {
        Ok(s) => toml::from_str(&s).map_err(|e| Error::Invalid(format!("{}: {e}", path.display()))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(e.into()),
    }
}

fn write_toml<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = toml::to_string_pretty(value).map_err(|e| Error::Invalid(e.to_string()))?;
    // Write atomically so a crash never leaves a truncated file.
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

/// Application preferences (independent from any controller profile).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub language: Lang,
    pub autostart: bool,
    pub close_to_tray: bool,
    pub start_hidden: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: Lang::from_env(),
            autostart: false,
            close_to_tray: true,
            start_hidden: false,
        }
    }
}

impl Settings {
    pub fn path() -> PathBuf {
        config_dir().join("settings.toml")
    }
    pub fn load() -> Self {
        read_toml(&Self::path()).unwrap_or_default()
    }
    pub fn save(&self) -> Result<()> {
        write_toml(&Self::path(), self)
    }
}

/// Hex encoding used to store raw profile bytes in TOML.
pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn from_hex(s: &str) -> Result<Vec<u8>> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if !s.len().is_multiple_of(2) {
        return Err(Error::Invalid("odd-length hex".into()));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| Error::Invalid("bad hex".into())))
        .collect()
}

/// Everything we can (re)apply to a controller.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DeviceConfig {
    /// Raw sealed profile, hex encoded.
    pub profile: Option<String>,
    pub polling_code: Option<u8>,
    pub led_brightness: Option<u8>,
    pub logo_colors: Option<[[u8; 3]; 3]>,
    /// Macro records by slot (hex encoded, sealed).
    pub macros: BTreeMap<u8, String>,
}

impl DeviceConfig {
    /// Factory state of a Rainbow 2 Pro (`280_v36`), as read from a new unit.
    pub fn factory() -> Self {
        use crate::protocol::macros::{Macro, SLOTS};
        DeviceConfig {
            profile: Some(to_hex(&crate::protocol::profile::RAINBOW2PRO_FACTORY)),
            polling_code: Some(0),
            led_brightness: Some(0xff),
            logo_colors: Some([[0; 3]; 3]),
            macros: (0..SLOTS as u8)
                .map(|s| (s, to_hex(&Macro::empty(s).encode().expect("empty macro"))))
                .collect(),
        }
    }

    /// Decoded macro records.
    pub fn macro_records(&self) -> Result<Vec<crate::protocol::macros::Macro>> {
        self.macros
            .values()
            .map(|h| from_hex(h).and_then(|b| crate::protocol::macros::Macro::decode(&b)))
            .collect()
    }

    pub fn profile(&self) -> Option<Result<Profile>> {
        self.profile
            .as_deref()
            .map(|h| from_hex(h).and_then(|b| Profile::from_bytes(&b)))
    }

    /// Fields of `self` that differ from `current` (what has to be sent).
    pub fn changes_from(&self, current: &DeviceConfig) -> DeviceConfig {
        fn pick<T: PartialEq + Clone>(want: &Option<T>, have: &Option<T>) -> Option<T> {
            match want {
                Some(w) if Some(w) != have.as_ref() => Some(w.clone()),
                _ => None,
            }
        }
        DeviceConfig {
            profile: pick(&self.profile, &current.profile),
            polling_code: pick(&self.polling_code, &current.polling_code),
            led_brightness: pick(&self.led_brightness, &current.led_brightness),
            logo_colors: pick(&self.logo_colors, &current.logo_colors),
            macros: self
                .macros
                .iter()
                .filter(|(slot, rec)| current.macros.get(slot) != Some(rec))
                .map(|(s, r)| (*s, r.clone()))
                .collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == DeviceConfig::default()
    }
}

/// Last configuration applied, per controller UUID.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub devices: BTreeMap<String, DeviceConfig>,
}

impl State {
    pub fn path() -> PathBuf {
        std::env::var_os("BIGBIGCTL_STATE")
            .map(PathBuf::from)
            .unwrap_or_else(|| config_dir().join("state.toml"))
    }
    pub fn load() -> Self {
        read_toml(&Self::path()).unwrap_or_default()
    }
    pub fn save(&self) -> Result<()> {
        write_toml(&Self::path(), self)
    }
}

/// A named profile file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NamedProfile {
    pub name: String,
    /// Model marker it was saved from (e.g. `ZJ-C1pro`).
    pub model: String,
    pub config: DeviceConfig,
}

pub fn profiles_dir() -> PathBuf {
    config_dir().join("profiles")
}

/// File-name-safe slug of a profile name.
pub fn slug(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() { "profile".into() } else { s }
}

pub fn list_profiles() -> Vec<NamedProfile> {
    let mut out: Vec<NamedProfile> = std::fs::read_dir(profiles_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "toml"))
        .filter_map(|e| read_toml::<NamedProfile>(&e.path()).ok())
        .filter(|p| !p.name.is_empty())
        .collect();
    out.sort_by_key(|p| p.name.to_lowercase());
    out
}

pub fn save_profile(p: &NamedProfile) -> Result<PathBuf> {
    let path = profiles_dir().join(format!("{}.toml", slug(&p.name)));
    write_toml(&path, p)?;
    Ok(path)
}

pub fn delete_profile(name: &str) -> Result<()> {
    let path = profiles_dir().join(format!("{}.toml", slug(name)));
    std::fs::remove_file(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        assert_eq!(
            from_hex(&to_hex(&[0, 1, 0xab, 0xff])).unwrap(),
            [0, 1, 0xab, 0xff]
        );
        assert!(from_hex("abc").is_err());
    }

    #[test]
    fn only_changed_parts_are_sent() {
        let current = DeviceConfig {
            profile: Some("aa".into()),
            polling_code: Some(0),
            led_brightness: Some(255),
            logo_colors: None,
            macros: BTreeMap::from([(0, "aa".into()), (1, "bb".into())]),
        };
        let wanted = DeviceConfig {
            profile: Some("aa".into()),
            polling_code: Some(1),
            led_brightness: Some(255),
            logo_colors: Some([[1, 2, 3]; 3]),
            macros: BTreeMap::from([(0, "aa".into()), (1, "cc".into())]),
        };
        let delta = wanted.changes_from(&current);
        assert_eq!(delta.profile, None);
        assert_eq!(delta.polling_code, Some(1));
        assert_eq!(delta.led_brightness, None);
        assert!(delta.logo_colors.is_some());
        assert_eq!(delta.macros, BTreeMap::from([(1, "cc".to_string())]));
        assert!(current.changes_from(&current).is_empty());
    }

    #[test]
    fn settings_toml_roundtrip() {
        let s = Settings {
            language: Lang::It,
            autostart: true,
            close_to_tray: false,
            start_hidden: true,
        };
        let back: Settings = toml::from_str(&toml::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
        // Unknown/missing fields fall back to defaults.
        let partial: Settings = toml::from_str("language = \"it\"").unwrap();
        assert_eq!(partial.language, Lang::It);
    }

    #[test]
    fn slugs_are_safe() {
        assert_eq!(slug("FPS / pro"), "FPS___pro");
        assert_eq!(slug(""), "profile");
    }
}
