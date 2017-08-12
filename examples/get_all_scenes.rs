extern crate philipshue;
extern crate tokio_core;
use std::env;
use philipshue::bridge::Bridge;
use philipshue::hue::AppData;

mod discover;
use discover::discover;

use tokio_core::reactor::Core;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Usage : {:?} <username>", args[0]);
        return;
    }
    let mut core = Core::new().unwrap();
    let bridge = Bridge::new(&core, discover().pop().unwrap(), &*args[1]);

    let all_scenes = core.run(bridge.get_all_scenes());
    match all_scenes {
        Ok(scenes) => {
            let name_len = std::cmp::max(4,
                scenes.values().map(|s| s.name.len()).max().unwrap_or(4)
            );
            let id_len = std::cmp::max(2,
                scenes.keys().map(|id| id.len()).max().unwrap_or(2)
            );
            println!("{0:2$} {1:3$} recycle locked appdata_and_version lights",
                     "id",
                     "name",
                     id_len,
                     name_len,
            );
            for (id, scene) in scenes.into_iter() {
                println!("{:id_len$} {:name_len$} {:7} {:6} {:20?} {:?}",
                         id,
                         scene.name,
                         scene.recycle,
                         scene.locked,
                         Show(scene.appdata.map(|AppData{data, version}| (data, version))),
                         scene.lights,
                         id_len = id_len,
                         name_len = name_len);
            }
        }
        Err(err) => println!("Error: {}", err),
    }
}

use std::fmt::{self, Debug, Display};

struct Show<T>(Option<T>);

impl<T: Debug> Debug for Show<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.0 {
            Some(ref x) => x.fmt(f),
            _ => Display::fmt("N/A", f),
        }
    }
}
