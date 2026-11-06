//! Hardware latency benchmark: round-trip timings on a connected BIGBIG WON
//! controller. **Read-only**: it only sends GET requests.
//!
//! Needs a Rainbow 2 Pro (or family) on `413d:2104`; `/dev/hidraw*` usually
//! requires sudo. Reports min / p50 / p99 / max per operation.
//!
//!   cargo build --release --no-default-features --example bench_io
//!   sudo ./target/release/examples/bench_io [rounds]

use std::time::{Duration, Instant};

use padtune::actions;
use padtune::device::Device;
use padtune::protocol::commands;
use padtune::protocol::frame;

fn stats(name: &str, mut v: Vec<Duration>) {
    if v.is_empty() {
        println!("{name:<36} (no samples)");
        return;
    }
    v.sort();
    let pick = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
    let ms = |d: Duration| d.as_secs_f64() * 1e3;
    println!(
        "{name:<36} n={:<3} min {:>7.2}  p50 {:>7.2}  p99 {:>7.2}  max {:>7.2} ms",
        v.len(),
        ms(v[0]),
        ms(pick(0.5)),
        ms(pick(0.99)),
        ms(v[v.len() - 1]),
    );
}

fn timed<T>(f: impl FnOnce() -> T) -> (Duration, T) {
    let t = Instant::now();
    let r = f();
    (t.elapsed(), r)
}

fn repeat(n: usize, mut f: impl FnMut() -> bool) -> Vec<Duration> {
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let (d, ok) = timed(&mut f);
        if ok {
            out.push(d);
        }
    }
    out
}

fn main() {
    let rounds: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(30);

    println!("== padtune I/O latency benchmark (read-only, {rounds} rounds) ==\n");

    let (t_open, dev) = timed(|| Device::open(None));
    let dev = match dev {
        Ok(d) => d,
        Err(e) => {
            eprintln!("cannot open controller: {e}");
            std::process::exit(1);
        }
    };
    println!(
        "device {:04x}:{:04x} {}  (open {:.2} ms)\n",
        dev.info.vendor_id,
        dev.info.product_id,
        dev.info.product,
        t_open.as_secs_f64() * 1e3
    );

    // 1. Pure drain cost: what every request pays before it even sends.
    stats(
        "drain() alone (idle bus)",
        repeat(rounds, || dev.drain().is_ok()),
    );

    // 2. Raw wire round-trip: send, then wait for the first matching frame,
    //    bypassing drain/retry so we see the controller's real reply time.
    let req = commands::get_zkm_version();
    let op = req[2];
    stats(
        "raw send->reply (ZkmVersion 0B)",
        repeat(rounds, || {
            if dev.send(&req).is_err() {
                return false;
            }
            let deadline = Instant::now() + Duration::from_secs(1);
            while Instant::now() < deadline {
                match dev.recv(Duration::from_millis(50)) {
                    Ok(Some(r)) => {
                        if frame::parse(&r).is_ok_and(|f| f.op() == op) {
                            return true;
                        }
                    }
                    Ok(None) => {}
                    Err(_) => return false,
                }
            }
            false
        }),
    );

    // 3. Full request() path (drain + send + collect + retries).
    stats(
        "request(ZkmVersion 0B)",
        repeat(rounds, || dev.request(&commands::get_zkm_version()).is_ok()),
    );
    stats(
        "request(GetMode E2)",
        repeat(rounds, || dev.request(&commands::get_mode()).is_ok()),
    );
    stats(
        "request(Power 04)",
        repeat(rounds, || dev.request(&commands::get_power()).is_ok()),
    );

    // 4. Blob read (profile, 5 long fragments).
    stats(
        "read_profile (D6 blob)",
        repeat(rounds.min(15), || actions::read_profile(&dev).is_ok()),
    );
    stats(
        "read_macro slot 0 (D9)",
        repeat(rounds.min(15), || actions::read_macro(&dev, 0).is_ok()),
    );

    // 5. Composite operations as the CLI/GUI run them.
    stats(
        "read_info (5 requests)",
        repeat(rounds.min(10), || actions::read_info(&dev).is_ok()),
    );
    stats(
        "read_snapshot (GUI refresh)",
        repeat(rounds.min(5), || actions::read_snapshot(&dev).is_ok()),
    );
}
