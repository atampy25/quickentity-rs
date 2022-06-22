mod qn_structs;
mod rt_structs;

use qn_structs::Entity;
use rt_structs::{PropertyID, RTBlueprint, RTTemplate, STemplateFactorySubEntity};

use serde_json::{from_slice, to_string, Value};
use std::{fs, io::Read};

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

fn read_as_rttemplate(path: &String) -> RTTemplate {
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

fn main() {
    let entity = read_as_rtblueprint(&String::from("entity.TBLU.json"));

    // let chars = fs::read_to_string(&String::from("entity.TEMP.json"))
    //     .unwrap()
    //     .chars()
    //     .collect::<Vec<char>>()[23850..23900]
    //     .to_vec()
    //     .iter()
    //     .collect::<String>();

    // dbg!(chars);

    // dbg!(entity
    //     .sub_entities
    //     .iter()
    //     .filter(
    //         |x| x.property_values.iter().any(|y| match &y.n_property_id {
    //             PropertyID::String(val) => val == "m_fValue",
    //             _ => false,
    //         })
    //     )
    //     .collect::<Vec<&STemplateFactorySubEntity>>());

    dbg!(entity);
}
