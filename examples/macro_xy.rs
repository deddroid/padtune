//! Hardware check: replace macro slot 0 (M1) with X 400 ms, pause, Y 400 ms.
use padtune::protocol::macros::Step;
fn main() -> padtune::Result<()> {
    let dev = padtune::device::Device::open(None)?;
    let mut m = padtune::actions::read_macro(&dev, 0)?;
    let step = |ms, inputs| Step {
        duration_ms: ms,
        marker: false,
        inputs,
        analog: [0; 4],
    };
    m.steps = vec![step(400, 1 << 3), step(400, 0), step(400, 1 << 4)];
    padtune::actions::write_macro(&dev, &m)?;
    println!(
        "written and verified: {:?}",
        padtune::actions::read_macro(&dev, 0)?.steps
    );
    Ok(())
}
