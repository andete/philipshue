extern crate philipshue;
extern crate tokio_core;

use std::env;
use std::num::ParseIntError;

use philipshue::bridge::Bridge;

mod discover;
use discover::discover;

use tokio_core::reactor::Core;

fn main() {
    match run() {
        Ok(()) => (),
        Err(_) => println!("Invalid number!"),
    }
}

fn run() -> Result<(), ParseIntError> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        println!("Usage: {} <username> <group_id> <scene_id>",
                 args[0]);
        return Ok(());
    }
    let mut core = Core::new().unwrap();
    let bridge = Bridge::new(&core, discover().pop().unwrap(), &*args[1]);
    let group_id: usize = args[2].parse()?;
    let scene = &*args[3];

    let recall = core.run(bridge.recall_scene_in_group(group_id, scene));
    match recall {
        Ok(resps) => {
            for resp in resps.into_iter() {
                println!("{:?}", resp)
            }
        }
        Err(e) => println!("Error occured when trying to send request:\n\t{}", e),
    }

    Ok(())
}
