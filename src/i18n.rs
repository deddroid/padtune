//! User-interface strings. English is the default; Italian is selectable.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    En,
    It,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::En, Lang::It];

    pub fn name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::It => "Italiano",
        }
    }

    /// Picks the language from the environment (`LANG`, `LC_ALL`), else English.
    pub fn from_env() -> Self {
        for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(v) = std::env::var(var)
                && !v.is_empty()
            {
                return if v.starts_with("it") { Lang::It } else { Lang::En };
            }
        }
        Lang::En
    }
}

macro_rules! strings {
    ($($key:ident => $en:literal, $it:literal;)*) => {
        /// Every translatable string, by key.
        #[allow(non_camel_case_types)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum T { $($key),* }

        impl T {
            pub const ALL: &'static [T] = &[$(T::$key),*];

            pub fn get(self, lang: Lang) -> &'static str {
                match (self, lang) {
                    $((T::$key, Lang::En) => $en, (T::$key, Lang::It) => $it,)*
                }
            }
        }
    };
}

strings! {
    AppTitle => "BIGBIG WON Controller", "Controller BIGBIG WON";
    TabDevice => "Device", "Dispositivo";
    TabButtons => "Buttons", "Tasti";
    TabSticks => "Sticks", "Stick";
    TabTriggers => "Triggers", "Grilletti";
    TabLights => "Lights", "Luci";
    TabMacros => "Macros", "Macro";
    TabProfiles => "Profiles", "Profili";
    TabSettings => "Settings", "Impostazioni";
    Connected => "Connected", "Connesso";
    Disconnected => "No controller", "Nessun controller";
    Connecting => "Connecting…", "Connessione…";
    Busy => "Working…", "Operazione in corso…";
    Refresh => "Refresh", "Aggiorna";
    Apply => "Apply to controller", "Applica al controller";
    Revert => "Discard changes", "Annulla modifiche";
    Unsaved => "unsaved changes", "modifiche non applicate";
    Model => "Model", "Modello";
    Firmware => "Firmware", "Firmware";
    Uuid => "Serial (UUID)", "Seriale (UUID)";
    ProfileFormat => "Profile format", "Formato profilo";
    Battery => "Battery", "Batteria";
    Connection => "Connection", "Connessione";
    Wired => "USB cable", "Cavo USB";
    Wireless => "2.4 GHz dongle", "Dongle 2,4 GHz";
    PollingRate => "Polling rate", "Frequenza di polling";
    PollingHint => "How often the controller reports its state to the PC.", "Quante volte al secondo il controller invia il suo stato al PC.";
    Backup => "Back up configuration…", "Backup configurazione…";
    Restore => "Restore from backup…", "Ripristina da backup…";
    BackupDone => "Backup saved", "Backup salvato";
    Button => "Button", "Tasto";
    Action => "Action", "Azione";
    Turbo => "Turbo", "Turbo";
    TurboSpeed => "Turbo speed", "Velocità turbo";
    MapDefault => "Default", "Predefinito";
    MapDisabled => "Disabled", "Disattivato";
    MapButton => "Controller button", "Tasto controller";
    MapKeyboard => "Keyboard key", "Tasto tastiera";
    MapRaw => "Other (preserved)", "Altro (preservato)";
    LeftStick => "Left stick", "Stick sinistro";
    RightStick => "Right stick", "Stick destro";
    CurveSet => "Curve set", "Set di curve";
    Deadzone => "Centre dead zone", "Zona morta centrale";
    AntiDeadzone => "Anti-dead zone", "Anti zona morta";
    OuterLimit => "Outer limit", "Limite esterno";
    MaxOutput => "Maximum output", "Uscita massima";
    PointA => "Point A", "Punto A";
    PointB => "Point B", "Punto B";
    Presets => "Presets", "Preset";
    PresetLinear => "Linear", "Lineare";
    PresetDelayed => "Delayed", "Ritardata";
    PresetPerformance => "Performance", "Prestazioni";
    PresetInstant => "Instant", "Istantanea";
    LeftTrigger => "Left trigger (LT)", "Grilletto sinistro (LT)";
    RightTrigger => "Right trigger (RT)", "Grilletto destro (RT)";
    TriggerStart => "Start (dead zone)", "Inizio (zona morta)";
    TriggerEnd => "Full press at", "Pressione piena a";
    HairTrigger => "Hair trigger", "Hair trigger (scatto immediato)";
    Brightness => "Brightness", "Luminosità";
    LogoColors => "Logo colour per mode", "Colore logo per modalità";
    ModeXbox => "Xbox / PC", "Xbox / PC";
    ModeSwitch => "Switch", "Switch";
    ModePs => "PlayStation", "PlayStation";
    MacroSlot => "Slot", "Slot";
    MacroEmpty => "empty", "vuoto";
    MacroSteps => "steps", "passi";
    ProfileName => "Profile name", "Nome profilo";
    SaveAsProfile => "Save current as profile", "Salva attuale come profilo";
    ApplyProfile => "Apply", "Applica";
    Delete => "Delete", "Elimina";
    NoProfiles => "No saved profiles yet.", "Nessun profilo salvato.";
    Language => "Language", "Lingua";
    Autostart => "Start with the system", "Avvia con il sistema";
    MinimizeToTray => "Close to tray", "Chiudi nella barra di sistema";
    StartHidden => "Start minimised in tray", "Avvia ridotto nella barra di sistema";
    Show => "Show", "Mostra";
    Quit => "Quit", "Esci";
    NotVerified => "Not yet verified on hardware", "Non ancora verificato sull'hardware";
    UdevHint => "Permission denied: install the udev rule (see README).", "Permesso negato: installa la regola udev (vedi README).";
    Done => "Done", "Fatto";
    Error => "Error", "Errore";
}

/// Shorthand: `tr(lang, T::Key)`.
pub fn tr(lang: Lang, key: T) -> &'static str {
    key.get(lang)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_string_is_translated() {
        for key in T::ALL {
            for lang in Lang::ALL {
                assert!(!key.get(lang).is_empty(), "{key:?} missing in {lang:?}");
            }
        }
    }
}
