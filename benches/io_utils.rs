use quickentity_rs::rpkg_structs::ResourceMeta;
use quickentity_rs::rt_structs::{RTBlueprint, RTFactory};

use serde_json::from_slice;

use std::{fs, io::Read};

pub fn read_as_rtfactory(path: &str) -> RTFactory {
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

pub fn read_as_rtblueprint(path: &str) -> RTBlueprint {
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

pub fn read_as_meta(path: &str) -> ResourceMeta {
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
