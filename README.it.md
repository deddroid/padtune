# bigbigctl

**Configura i controller BIGBIG WON (Rainbow 2 Pro e famiglia) su Linux e Windows** — senza software ufficiale.

[Read in English](README.md)

![Tasti](docs/screenshots/buttons-it.png)

bigbigctl è un unico programma in Rust con un'interfaccia grafica
(`bigbigctl-gui`) e uno strumento da riga di comando (`bigbigctl`). Dialoga con
l'interfaccia di configurazione del controller, quindi ogni impostazione resta
**salvata nel controller** e funziona poi su qualsiasi PC o console.

## Funzioni

| | |
|---|---|
| **Tasti** | Rimappa qualsiasi tasto su un altro tasto o su un tasto della tastiera (con modificatori), disattivalo, **turbo** per tasto e velocità del turbo. Clicca il tasto sul disegno del controller. |
| **Stick** | Editor della curva (zona morta, anti zona morta, due punti, limite esterno, uscita massima), preset, tre set di curve, posizione dello stick in tempo reale. |
| **Grilletti** | Inizio della zona morta, punto di pressione piena, hair trigger, corsa in tempo reale. |
| **Luci** | Luminosità della striscia LED, colore del logo HOME con intensità, ritorno ai valori di fabbrica con un clic. |
| **Macro** | Modifica le quattro macro del controller passo per passo (tasti premuti, durata) e associale a M1/M2. |
| **Polling** | Lettura e impostazione a 250 / 500 / 1000 / 2000 Hz. |
| **Dispositivo** | Modello, firmware, seriale, batteria, cavo USB o dongle 2.4G, test tasti (tutti, anche M1/M2, FN e i tasti frontali), backup/ripristino, impostazioni di fabbrica. |
| **Profili** | Salva profili con nome e applicali con un clic — vengono inviate solo le parti cambiate. |
| **Funzioni del controller** | Cosa fa ogni scorciatoia integrata (TURBO, Mapping, MACRO, Configuration, combinazioni con FN, calibrazione, cambio modalità), dal manuale ufficiale. |
| **Desktop** | Icona nella barra di sistema, avvio con il sistema, istanza unica, inglese e italiano. |

## Compatibilità

| Modello | ID USB | Firmware / marker | Profilo | Stato |
|---|---|---|---|---|
| Rainbow 2 Pro (cavo) | `413d:2104` | ZKM 6, `ZJ-C1pro` | `280_v36` | **Provato**: tutto quanto sopra |
| Rainbow 2 Pro (dongle 2.4G) | `413d:2104` | uguale | `280_v36` | Letture provate; test tasti non disponibile via dongle |
| Rainbow 2 SE, Rainbow 3, Blitz2, Gale… | `413d:2104` | altri | `240`–`508` | Stessa famiglia di protocollo, **non provati** — la lettura dovrebbe funzionare; le scritture sono rifiutate per layout di profilo sconosciuti |

Hai un altro modello? Apri una issue con l'output di `bigbigctl list` e
`bigbigctl info` (il modello di issue lo chiede).

## Installazione

### Arch Linux (AUR)

```sh
yay -S bigbigctl        # compila dai sorgenti
yay -S bigbigctl-bin    # binari precompilati
```

Il pacchetto installa la regola udev, così il controller funziona senza root.

### Altre distribuzioni Linux

Scarica `bigbigctl-<versione>-linux-x86_64.tar.gz` dalle
[Release](https://github.com/deddroid/bigbigctl/releases), estrailo ed esegui:

```sh
./install.sh --udev     # installa in ~/.local; --udev aggiunge la regola udev (sudo)
./install.sh --uninstall
```

### Windows

Scarica `bigbigctl-<versione>-windows-x86_64.zip` dalle
[Release](https://github.com/deddroid/bigbigctl/releases), estrailo e avvia
`bigbigctl-gui.exe`. Non servono driver. Gli eseguibili non sono firmati,
quindi SmartScreen può avvisare al primo avvio ("Ulteriori informazioni" →
"Esegui comunque").

### Dai sorgenti

```sh
cargo build --release                       # GUI + CLI
cargo build --release --no-default-features # solo CLI (senza dipendenze grafiche)
```

Su Linux servono `hidapi` (hidraw) e, per la GUI, un driver OpenGL funzionante.

## Uso

Avvia **BIGBIG WON Controller** dal menu, oppure `bigbigctl-gui`. Modifica le
impostazioni e premi **Applica al controller**: viene scritto solo ciò che è
cambiato, e ogni scrittura viene verificata rileggendola.

```text
bigbigctl list                      # controller collegati
bigbigctl info                      # modello, firmware, polling, batteria…
bigbigctl profile                   # profilo attuale
bigbigctl backup mio.bin            # salva il profilo
bigbigctl restore mio.bin           # lo riscrive (verificato)
bigbigctl macros                    # slot macro con i loro passi
bigbigctl set polling 1000
bigbigctl set brightness 128        # striscia LED, 0-255
bigbigctl set logo ff0000           # colore logo HOME (000000 = fabbrica)
bigbigctl set trigger left --start 5 --end 90 --hair false
bigbigctl monitor 30                # test tasti: stampa ogni pressione
```

`bigbigctl-gui --hidden` si avvia nella barra di sistema.

### Variabili d'ambiente

| Variabile | Effetto |
|---|---|
| `BIGBIGCTL_DEBUG=1` | Stampa ogni report inviato e ricevuto |
| `BIGBIGCTL_CONFIG_DIR` | Usa un'altra cartella per impostazioni e profili |
| `BIGBIGCTL_STATE` | Usa un altro file di stato (ultima configurazione applicata) |
| `BIGBIGCTL_DEMO=1` | GUI con un Rainbow 2 Pro simulato, senza accesso USB |
| `BIGBIGCTL_SCREENSHOTS=cartella` | Salva uno screenshot di ogni scheda in ogni lingua, poi esce |

Le impostazioni sono in `~/.config/bigbigctl/` (Linux) o `%APPDATA%\bigbigctl\`
(Windows): `settings.toml` (preferenze dell'app), `state.toml` (ultima
configurazione applicata per controller) e `profiles/*.toml`.

## Limiti noti

* Il **test tasti** mette in pausa il gamepad mentre è attivo (è il
  funzionamento del controller) e funziona solo via cavo USB.
* La **configurazione integrata attiva** (i quattro LED bianchi) non è
  leggibile dal controller.
* Il **colore personalizzato della striscia LED** (doppio clic su FN) non è
  ancora impostabile; luminosità e colore del logo HOME sì.
* Le impostazioni di vibrazione e controllo di movimento vengono preservate ma
  non sono ancora modificabili.
* Le versioni Windows sono prodotte dalla CI e non sono state provate
  dall'autore su un PC Windows reale.

Dettagli del protocollo: [docs/PROTOCOL.md](docs/PROTOCOL.md).

## Crediti

* **deddroid** — autore, test sull'hardware.
* **Claude** (Anthropic) — reverse engineering e codice, scritto con deddroid.
* [BlitzForge](https://github.com/tikokuma/BlitzForge) di tikokuma,
  [Better BigBig Won Assistant](https://github.com/XBXSean/-BBBWA-BetterBigBigWonAssistant-) di XBXSean,
  [armorx-toolkit](https://github.com/feshinkof-boop/armorx-toolkit) di feshinkof-boop —
  ricerche sul protocollo su cui si basa questo progetto.
* [REA](https://github.com/morluto/rea) e [Ghidra](https://ghidra-sre.org/) per l'analisi.

Progetto non affiliato a BIGBIG WON. I nomi dei prodotti sono marchi dei rispettivi proprietari.

## Licenza

[MIT](LICENSE)
