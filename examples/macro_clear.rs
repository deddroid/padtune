//! Restore a macro slot to its factory (empty, unbound) state.
fn main() -> bigbigctl::Result<()> {
    let slot: u8 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let dev = bigbigctl::device::Device::open(None)?;
    bigbigctl::actions::write_macro(&dev, &bigbigctl::protocol::macros::Macro::empty(slot))?;
    println!("slot {slot} cleared and verified");
    Ok(())
}
