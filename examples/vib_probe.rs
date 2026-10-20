//! Hardware check: write one vibration change and show what reads back.
use padtune::protocol::profile::{Profile, Vibration};
fn main() -> padtune::Result<()> {
    let args: Vec<u8> = std::env::args()
        .skip(1)
        .filter_map(|a| a.parse().ok())
        .collect();
    let (motor, min, max) = (args[0] as usize, args[1], args[2]);
    let dev = padtune::device::Device::open(None)?;
    let before = padtune::actions::read_profile(&dev)?;
    let mut p = before.clone();
    p.set_vibration(motor, Vibration { min, max })?;
    let status = dev.send_blob(&p.write_frames())?;
    let back = padtune::actions::read_profile(&dev)?;
    println!("status {status}");
    for (i, a, b) in back.diff(&p) {
        println!("diff at 0x{i:02x}: wrote {b:02x} read {a:02x}");
    }
    println!("vibration raw now {:02x?}", back.vibration_raw());
    let _ = Profile::from_bytes(&back.to_bytes())?;
    Ok(())
}
