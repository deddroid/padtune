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
                return if v.starts_with("it") {
                    Lang::It
                } else {
                    Lang::En
                };
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
    Wired => "USB", "USB";
    Wireless => "Dongle 2.4G", "Dongle 2,4G";
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
    Brightness => "Light strip brightness", "Luminosità striscia LED";
    BrightnessHint => "Dims the RGB strip around the controller (0 = off). Logo and profile LEDs are not affected.", "Regola la striscia RGB attorno al controller (0 = spenta). Logo e LED dei profili non cambiano.";
    Connections => "Connections", "Connessioni";
    KeyTest => "Key test", "Test tasti";
    KeyTestOff => "Shows every button (M1, M2, front keys, FN). Pauses the gamepad while on.", "Mostra tutti i tasti (M1, M2, frontali, FN). Mentre è attivo il gamepad è in pausa.";
    KeyTestOn => "Gamepad paused — sticks are not shown. Turns off when you leave this tab.", "Gamepad in pausa — gli stick non vengono mostrati. Si spegne lasciando questa scheda.";
    ModeLightHint => "In “mode light” (default) the strip and the Home light share the mode colour: green in XInput, red in Switch. Double-click FN cycles mode light / custom light / off.", "In “luce modalità” (predefinita) striscia e luce Home hanno il colore della modalità: verde in XInput, rosso in Switch. Doppio clic su FN alterna luce modalità / personalizzata / spenta.";
    NotConnected => "not connected", "non collegato";
    Active => "active", "attiva";
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
    PadHint => "Click a button to edit it. T = turbo, ↻ = remapped. Pressed buttons light up live.", "Clicca un tasto per modificarlo. T = turbo, ↻ = rimappato. I tasti premuti si illuminano in tempo reale.";
    TurboHint => "Turbo repeats the button while it is held.", "Il turbo ripete il tasto finché lo tieni premuto.";
    LiveInput => "Live input", "Input in tempo reale";
    Functions => "Controller functions", "Funzioni del controller";
    FunctionsHint => "On-board features started from the controller itself (from the official manual).", "Funzioni integrate, avviate dal controller stesso (dal manuale ufficiale).";
    FunctionKey => "Function key — not remappable", "Tasto funzione — non rimappabile";
    HowToStart => "How to start", "Come si avvia";
    ToConfirm => "key assignment to be confirmed", "assegnazione del tasto da confermare";
    FTurboT => "Turbo", "Turbo";
    FTurboD => "Repeats A/B/X/Y, D-pad, LB/RB/LT/RT or M1–M4 while held; the FN light blinks at the turbo rate. Clear one: hold the button and double-click TURBO. Clear all: double-click TURBO. FN + ← cycles 5 / 10 / 20 Hz.", "Ripete A/B/X/Y, croce, LB/RB/LT/RT o M1–M4 finché sono premuti; la luce FN lampeggia alla frequenza del turbo. Cancella uno: tieni il tasto e doppio clic su TURBO. Cancella tutti: doppio clic su TURBO. FN + ← cambia 5 / 10 / 20 Hz.";
    FTurboC => "Hold TURBO + the button", "Tieni TURBO + il tasto";
    FMapT => "Back-button mapping", "Mappatura tasti posteriori";
    FMapD => "Makes a back button copy another button without the app; the FN light breathes, then blinks twice when done. Clear one: hold M1/M2 and double-click Mapping. Clear all: double-click Mapping.", "Fa copiare a un tasto posteriore un altro tasto senza app; la luce FN pulsa, poi lampeggia due volte a fine operazione. Cancella uno: tieni M1/M2 e doppio clic su Mapping. Cancella tutti: doppio clic su Mapping.";
    FMapC => "Mapping + M1/M2, then the button to copy", "Mapping + M1/M2, poi il tasto da copiare";
    FMacroT => "Macro recording", "Registrazione macro";
    FMacroD => "Records a sequence on a back button: press the keys, then MACRO to finish. FN breathes while recording and blinks slowly while the macro plays. Clear one: press M1/M2 and double-click MACRO. Clear all: double-click MACRO.", "Registra una sequenza su un tasto posteriore: premi i tasti, poi MACRO per terminare. FN pulsa durante la registrazione e lampeggia lenta durante la riproduzione. Cancella una: premi M1/M2 e doppio clic su MACRO. Cancella tutte: doppio clic su MACRO.";
    FMacroC => "Hold MACRO + M1/M2 for 3 s", "Tieni MACRO + M1/M2 per 3 s";
    FProfileT => "On-board profiles", "Profili integrati";
    FProfileD => "Four configurations stored in the controller; in XInput mode the white LEDs show the active one (top = 1). Holding the key for 3 s clears the current configuration.", "Quattro configurazioni salvate nel controller; in modalità XInput i LED bianchi indicano quella attiva (in alto = 1). Tenendo premuto 3 s si cancella la configurazione corrente.";
    FProfileC => "Press Configuration", "Premi Configuration";
    FLightT => "Light mode", "Modalità luci";
    FLightD => "Cycles the light strip between mode colour, custom colour and off. Brightness is set in the Lights tab.", "Alterna la striscia luminosa tra colore di modalità, colore personalizzato e spenta. La luminosità si imposta nella scheda Luci.";
    FLightC => "Double-click FN", "Doppio clic su FN";
    FVibT => "Trigger vibration", "Vibrazione grilletti";
    FVibD => "Cycles the trigger motors: strong, medium, weak, off.", "Alterna i motori dei grilletti: forte, media, debole, spenta.";
    FVibC => "FN + →", "FN + →";
    FVolT => "PC volume", "Volume del PC";
    FVolD => "Raises or lowers the system volume.", "Alza o abbassa il volume di sistema.";
    FVolC => "FN + ↑ / FN + ↓", "FN + ↑ / FN + ↓";
    FGyroT => "Motion control", "Controllo di movimento";
    FGyroD => "The gyroscope moves the left or right stick (global), or the right stick only while a trigger is held (response mode).", "Il giroscopio muove lo stick sinistro o destro (globale), oppure il destro solo tenendo un grilletto (modalità risposta).";
    FGyroC => "FN + L3 · FN + R3 · FN + trigger + R3", "FN + L3 · FN + R3 · FN + grilletto + R3";
    FModeT => "Console mode", "Modalità console";
    FModeD => "Switches between Nintendo Switch mode (red Home light) and XInput / PC mode (green Home light).", "Passa tra modalità Nintendo Switch (luce Home rossa) e XInput / PC (luce Home verde).";
    FModeC => "A + Home 3 s (Switch) · B + Home 3 s (XInput)", "A + Home 3 s (Switch) · B + Home 3 s (XInput)";
    FCalT => "Calibration", "Calibrazione";
    FCalD => "Sticks and triggers: orange breathing until done. Gyroscope: keep the controller flat, purple breathing until done.", "Stick e grilletti: luce arancione pulsante fino al termine. Giroscopio: controller in piano, luce viola pulsante fino al termine.";
    FCalC => "View + Menu 3 s · Menu + Share (gyro)", "View + Menu 3 s · Menu + Share (giroscopio)";
    LiveUnavailable => "Live input not available (gamepad not visible to the system).", "Input live non disponibile (gamepad non visibile al sistema).";
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
