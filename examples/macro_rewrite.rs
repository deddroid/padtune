//! Hardware check: re-write macro slot N unchanged and verify (D8 + D9).
fn main() -> bigbigctl::Result<()> {
    let slot: u8 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let dev = bigbigctl::device::Device::open(None)?;
    let m = bigbigctl::actions::read_macro(&dev, slot)?;
    println!(
        "slot {slot}: {} steps, record {:02x?}",
        m.steps.len(),
        &m.encode()?[..10]
    );
    bigbigctl::actions::write_macro(&dev, &m)?;
    println!("re-written and verified");
    Ok(())
}
