//! Prints the live input state for a few seconds (manual check of gilrs).
fn main() {
    let state = bigbigctl::gui::live::spawn(|| {});
    for _ in 0..12 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        println!("{:?}", state.lock().unwrap());
    }
}
