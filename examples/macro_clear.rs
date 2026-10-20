//! Restore a macro slot to its factory (empty, unbound) state.
fn main() -> padtune::Result<()> {
    let slot: u8 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let dev = padtune::device::Device::open(None)?;
    padtune::actions::write_macro(&dev, &padtune::protocol::macros::Macro::empty(slot))?;
    println!("slot {slot} cleared and verified");
    Ok(())
}
