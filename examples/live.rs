//! Prints the live input state for a few seconds (manual check of gilrs).
fn main() {
    let live = padtune::gui::live::spawn(|| {});
    for _ in 0..12 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        println!("{:?}", live.state.lock().unwrap());
    }
}
