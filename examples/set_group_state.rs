extern crate philipshue;
extern crate tokio_core;

use std::env;
use std::num::ParseIntError;

use philipshue::hue::LightCommand;
use philipshue::bridge::Bridge;

use tokio_core::reactor::Core;

mod discover;
use discover::{discover, rgb_to_hsv};

fn main() {
    match run() {
        Ok(()) => (),
        Err(_) => println!("Invalid number!"),
    }
}

fn run() -> Result<(), ParseIntError> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        println!("Usage: {} <username> <group_id> on|off|bri <bri>|hue <hue>|sat \
                  <sat>|rgb <r> <g> <b>|hsv <hue> <sat> <bri>|mired <ct> <bri>|kelvin <temp> \
                  <bri>",
                 args[0]);
        return Ok(());
    }
    let mut core = Core::new().unwrap();
    let bridge = Bridge::new(&core, discover().pop().unwrap(), &*args[1]);
    let group_id: usize = args[2].parse()?;

    let cmd = LightCommand::default();

    let cmd = match &*args[3] {
        "on" => cmd.on(),
        "off" => cmd.off(),
        "bri" => cmd.with_bri(args[4].parse()?),
        "hue" => cmd.with_hue(args[4].parse()?),
        "sat" => cmd.with_sat(args[4].parse()?),
        "hsv" => {
            cmd.with_hue(args[4].parse()?)
                .with_sat(args[5].parse()?)
                .with_bri(args[6].parse()?)
        }
        "rgb" => {
            let (hue, sat, bri) = rgb_to_hsv(args[4].parse()?, args[5].parse()?, args[6].parse()?);
            cmd.with_hue(hue).with_sat(sat).with_bri(bri)
        }
        "mired" => {
            cmd.with_ct(args[4].parse()?)
                .with_bri(args[5].parse()?)
                .with_sat(254)
        }
        "kelvin" => {
            cmd.with_ct((1000000u32 / args[4].parse::<u32>()?) as u16)
                .with_bri(args[5].parse()?)
                .with_sat(254)
        }
        _ => return Ok(println!("Invalid command!")),
    };

    let resp = core.run(bridge.set_group_state(group_id, &cmd));
    match resp {
        Ok(resps) => {
            for resp in resps.into_iter() {
                println!("{:?}", resp)
            }
        }
        Err(e) => println!("Error occured when trying to send request:\n\t{}", e),
    }

    Ok(())
}
