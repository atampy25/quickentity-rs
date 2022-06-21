mod structs;

use std::{fs, io::Read};

use serde_json::{from_slice, to_string, Value};
use structs::Entity;

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

fn main() {
    let entity: Entity = read_as_entity(&String::from("entity.json"));

    for (entity_id, entity_data) in entity.entities.iter() {
        println!("{}", entity_id);
        println!("{:?}", entity_data);
    }
}
