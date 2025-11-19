use std::panic;
use std::thread::sleep;

pub fn init_ungraceful_panic_handler() {
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        default_hook(panic_info);
        println!("An unexpected condition (panic) has occurred.");
        println!("Exiting...");
        sleep(std::time::Duration::from_secs(2));
        std::process::exit(1);
    }));
}
