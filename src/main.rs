mod qn_structs;
mod quickentity;
mod rpkg_structs;
mod rt_structs;
mod util_structs;

use qn_structs::Entity;
use rpkg_structs::ResourceMeta;
use rt_structs::{RTBlueprint, RTFactory};

use serde_json::{from_slice, to_vec, to_vec_pretty, Value};
use std::time::Instant;
use std::{
    fs::{self, File},
    io::Read,
};

use crate::quickentity::{convert_to_qn, Game};

fn read_as_json(path: &String) -> Value {
    from_slice(&{
        let mut vec = Vec::new();
        fs::File::open(path)
            .expect("Failed to open file")
            .read_to_end(&mut vec)
            .expect("Failed to read file");
        vec
    })
    .expect("Failed to open file as JSON")
}

fn read_as_entity(path: &String) -> Entity {
    from_slice(&{
        let mut vec = Vec::new();
        fs::File::open(path)
            .expect("Failed to open file")
            .read_to_end(&mut vec)
            .expect("Failed to read file");
        vec
    })
    .expect("Failed to open file as JSON")
}

fn read_as_rtfactory(path: &String) -> RTFactory {
    from_slice(&{
        let mut vec = Vec::new();
        fs::File::open(path)
            .expect("Failed to open file")
            .read_to_end(&mut vec)
            .expect("Failed to read file");
        vec
    })
    .expect("Failed to open file as JSON")
}

fn read_as_rtblueprint(path: &String) -> RTBlueprint {
    from_slice(&{
        let mut vec = Vec::new();
        fs::File::open(path)
            .expect("Failed to open file")
            .read_to_end(&mut vec)
            .expect("Failed to read file");
        vec
    })
    .expect("Failed to open file as JSON")
}

fn read_as_meta(path: &String) -> ResourceMeta {
    from_slice(&{
        let mut vec = Vec::new();
        fs::File::open(path)
            .expect("Failed to open file")
            .read_to_end(&mut vec)
            .expect("Failed to read file");
        vec
    })
    .expect("Failed to open file as JSON")
}

fn main() {
    let now = Instant::now();

    let entity = convert_to_qn(
        &read_as_rtfactory(&String::from("0056406088754691.TEMP.json")),
        &read_as_meta(&String::from("0056406088754691.TEMP.meta.JSON")),
        &read_as_rtblueprint(&String::from("00CF14C55C3BCCA8.TBLU.json")),
        &read_as_meta(&String::from("00CF14C55C3BCCA8.TBLU.meta.JSON")),
        Game::HM3,
    );

    // dbg!(&entity);

    fs::write("entity.json", to_vec_pretty(&entity).unwrap()).unwrap();

    let elapsed = now.elapsed();
    println!("Elapsed: {:.2?}", elapsed);
}
