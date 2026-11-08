//! Software-only micro-benchmark of the protocol layer (no hardware).
//!
//! Measures the pure CPU cost of framing, CRC, profile/macro codecs and the
//! hex/config paths the GUI evaluates every frame (`dirty()` compares two
//! `Snapshot::config()`). Reports the median ns/op over several rounds.
//!
//!   cargo run --release --no-default-features --example bench_proto

use std::hint::black_box;
use std::time::Instant;

use padtune::config::{DeviceConfig, from_hex, to_hex};
use padtune::protocol::commands;
use padtune::protocol::crc::crc16_modbus;
use padtune::protocol::frame::{self, long_frames, parse, short};
use padtune::protocol::macros::{Macro, Step};
use padtune::protocol::profile::{Profile, RAINBOW2PRO_FACTORY};

const ROUNDS: usize = 7;

/// Runs `f` `iters` times per round, returns the median per-op nanoseconds.
fn bench(name: &str, iters: u32, mut f: impl FnMut()) {
    // Warm-up.
    for _ in 0..iters.min(1000) {
        f();
    }
    let mut samples: Vec<f64> = (0..ROUNDS)
        .map(|_| {
            let t = Instant::now();
            for _ in 0..iters {
                f();
            }
            t.elapsed().as_nanos() as f64 / f64::from(iters)
        })
        .collect();
    samples.sort_by(f64::total_cmp);
    let med = samples[ROUNDS / 2];
    let min = samples[0];
    println!("{name:<44} {med:>10.1} ns/op   (min {min:.1})");
}

fn sample_macro() -> Macro {
    let mut m = Macro::empty(1);
    m.run_key = 24;
    m.repeat = 3;
    m.steps = vec![
        Step {
            duration_ms: 96,
            marker: false,
            inputs: 2,
            analog: [0; 4],
        },
        Step {
            duration_ms: 32_760,
            marker: true,
            inputs: 0,
            analog: [-128, 127, 0, -1],
        },
    ];
    m
}

fn main() {
    let profile = Profile::from_bytes(&RAINBOW2PRO_FACTORY).expect("factory profile is valid");
    let profile_bytes = profile.to_bytes();
    let profile_hex = to_hex(&profile_bytes);
    let macro_rec = sample_macro().encode().expect("sample macro is valid");
    let long = long_frames(commands::op::SET_PROFILE, &profile_bytes);
    let e2_reply = [
        0xa5u8, 0x10, 0xe2, 0x56, 0x51, 0x01, 0x5a, 0x4a, 0x2d, 0x43, 0x31, 0x70, 0x72, 0x6f, 0x00,
        0xd5,
    ];
    let factory_cfg = DeviceConfig::factory();
    let mut changed_cfg = factory_cfg.clone();
    changed_cfg.polling_code = Some(1);

    println!("== padtune protocol micro-benchmark (median of {ROUNDS} rounds) ==\n");

    bench("frame::short (build 64B request)", 200_000, || {
        black_box(short(black_box(commands::op::GET_MODE), black_box(&[])));
    });
    bench("frame::parse (short reply)", 200_000, || {
        black_box(parse(black_box(&e2_reply)).expect("captured reply is valid"));
    });
    bench("frame::long_frames (280B -> 5 frames)", 50_000, || {
        black_box(long_frames(
            commands::op::SET_PROFILE,
            black_box(&profile_bytes),
        ));
    });
    bench("frame::reassemble (5 frames -> 280B)", 50_000, || {
        let parsed: Vec<_> = long
            .iter()
            .map(|f| parse(f).expect("own frames are valid"))
            .collect();
        black_box(
            frame::reassemble(commands::op::SET_PROFILE, &parsed).expect("fragments are complete"),
        );
    });
    bench("crc16_modbus (280B)", 50_000, || {
        black_box(crc16_modbus(black_box(&profile_bytes)));
    });
    bench("Profile::from_bytes (280B, CRC check)", 50_000, || {
        black_box(Profile::from_bytes(black_box(&profile_bytes)).expect("sealed profile is valid"));
    });
    bench("Profile::to_bytes (clone + seal)", 50_000, || {
        black_box(black_box(&profile).to_bytes());
    });
    bench("Macro::decode (2 steps)", 100_000, || {
        black_box(Macro::decode(black_box(&macro_rec)).expect("sealed macro is valid"));
    });
    bench("Macro::encode (2 steps)", 100_000, || {
        black_box(
            black_box(&sample_macro())
                .encode()
                .expect("sample macro is valid"),
        );
    });
    bench("to_hex (280B profile)", 50_000, || {
        black_box(to_hex(black_box(&profile_bytes)));
    });
    bench("from_hex (560 chars)", 50_000, || {
        black_box(from_hex(black_box(&profile_hex)).expect("hex came from to_hex"));
    });
    bench("DeviceConfig::changes_from (factory)", 20_000, || {
        black_box(black_box(&changed_cfg).changes_from(black_box(&factory_cfg)));
    });
    bench("DeviceConfig == (GUI dirty() compare)", 50_000, || {
        black_box(black_box(&changed_cfg) == black_box(&factory_cfg));
    });
    // What the GUI pays per frame in `dirty()`: two full config() builds.
    let macros: Vec<Macro> = (0..4).map(Macro::empty).collect();
    bench("snapshot config() build (profile+4 macros)", 20_000, || {
        let cfg = DeviceConfig {
            profile: Some(to_hex(&profile.to_bytes())),
            polling_code: Some(0),
            led_brightness: Some(255),
            logo_colors: Some([[0; 3]; 3]),
            macros: macros
                .iter()
                .filter_map(|m| m.encode().ok().map(|r| (m.slot, to_hex(&r))))
                .collect(),
            strip: None,
        };
        black_box(cfg);
    });
    bench("short frame build+checksum (strip set)", 100_000, || {
        black_box(commands::set_logo_led(black_box([
            [1, 2, 3],
            [4, 5, 6],
            [7, 8, 9],
        ])));
    });
}
