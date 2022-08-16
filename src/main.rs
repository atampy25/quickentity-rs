mod qn_structs;
mod quickentity;
mod rpkg_structs;
mod rt_structs;
mod util_structs;

use qn_structs::Entity;
use rpkg_structs::ResourceMeta;
use rt_structs::{RTBlueprint, RTFactory};

use serde_json::{from_slice, to_vec, Value};
use std::time::{Instant, SystemTime};
use std::{fs, io::Read};

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

	let fac = read_as_rtfactory(
		&fs::read_dir("corpus\\items colombia")
			.unwrap()
			.find(|x| {
				x.as_ref()
					.unwrap()
					.file_name()
					.to_str()
					.unwrap()
					.ends_with("TEMP.json")
			})
			.unwrap()
			.unwrap()
			.path()
			.as_os_str()
			.to_str()
			.unwrap()
			.to_string(),
	);
	let fac_meta = read_as_meta(&String::from(
		&fs::read_dir("corpus\\items colombia")
			.unwrap()
			.find(|x| {
				x.as_ref()
					.unwrap()
					.file_name()
					.to_str()
					.unwrap()
					.ends_with("TEMP.meta.JSON")
			})
			.unwrap()
			.unwrap()
			.path()
			.as_os_str()
			.to_str()
			.unwrap()
			.to_string(),
	));
	let blu = read_as_rtblueprint(
		&fs::read_dir("corpus\\items colombia")
			.unwrap()
			.find(|x| {
				x.as_ref()
					.unwrap()
					.file_name()
					.to_str()
					.unwrap()
					.ends_with("TBLU.json")
			})
			.unwrap()
			.unwrap()
			.path()
			.as_os_str()
			.to_str()
			.unwrap()
			.to_string(),
	);
	let blu_meta = read_as_meta(
		&fs::read_dir("corpus\\items colombia")
			.unwrap()
			.find(|x| {
				x.as_ref()
					.unwrap()
					.file_name()
					.to_str()
					.unwrap()
					.ends_with("TBLU.meta.JSON")
			})
			.unwrap()
			.unwrap()
			.path()
			.as_os_str()
			.to_str()
			.unwrap()
			.to_string(),
	);

	let entity = timeit(|| convert_to_qn(&fac, &fac_meta, &blu, &blu_meta, Game::HM3));

	// dbg!(&entity);

	fs::write("entity.json", to_vec(&entity).unwrap()).unwrap();

	let elapsed = now.elapsed();
	println!("Elapsed: {:.2?}", elapsed);
}

fn timeit<F: FnMut() -> T, T>(mut f: F) -> T {
	let start = SystemTime::now();
	let result = f();
	let end = SystemTime::now();
	let duration = end.duration_since(start).unwrap();
	println!("Fn took {} milliseconds", duration.as_millis());
	result
}
