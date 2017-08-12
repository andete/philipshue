extern crate philipshue;
extern crate tokio_core;
extern crate tokio_timer;
extern crate futures;

use std::env;
use std::time::Duration;
use std::num::ParseIntError;

use philipshue::hue::LightCommand;
use philipshue::bridge::Bridge;

use tokio_core::reactor::Core;
use futures::{Future, Stream};

mod discover;
use discover::{discover, rgb_to_hsv};

fn main() {
    match run() {
        Ok(()) => (),
        Err(_) => println!("Invalid number!"),
    }
}

fn run() -> std::result::Result<(), ParseIntError> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        println!("Usage: {} <username> <light_id>,<light_id>,... on|off|bri <bri>|hue <hue>|sat <sat>|rgb <r> <g> <b>|hsv <hue> <sat> <bri>|mired \
                  <ct> <bri>|kelvin <temp> <bri>",
                 args[0]);
        return Ok(());
    }
    let mut core = Core::new().unwrap();
    let bridge = Bridge::new(&core, discover().pop().unwrap(), &*args[1]);
    let input_lights = args[2].split(",")
        .fold(Ok(Vec::new()),
              |v, s| v.and_then(|mut v| s.parse::<usize>().map(|n| v.push(n)).map(|_| v)))?;

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

    let stream = futures::stream::iter(input_lights.into_iter().map(|l| Ok(l)));
    let future = stream.for_each(|id| {
        bridge.set_light_state(id, &cmd)
            .map_err(|e| {
                println!("Error occured when trying to send request:\n\t{}", e);
                e
            })
            .and_then(|resps| {
                for resp in resps.into_iter() {
                    println!("{:?}", resp)
                }
                let timer = tokio_timer::Timer::default();
                let sleep = timer.sleep(Duration::from_millis(50));
                sleep.wait().map_err(From::from)
            })
    });
    core.run(future).unwrap();
    Ok(())
}
