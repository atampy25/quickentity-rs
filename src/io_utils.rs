use quickentity_rs::qn_structs::Entity;
use quickentity_rs::rpkg_structs::ResourceMeta;
use quickentity_rs::rt_structs::{RTBlueprint, RTFactory};
use quickentity_rs::{convert_2016_blueprint_to_modern, convert_2016_factory_to_modern};

use serde::Serialize;
use serde_json::ser::Formatter;
use serde_json::{from_slice, from_value, Serializer, Value};
use std::io;
use std::{fs, io::Read};

pub fn read_as_value(path: &str) -> Value {
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

pub fn read_as_entity(path: &str) -> Entity {
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

pub fn read_as_rtfactory(path: &str) -> RTFactory {
	let val = from_slice::<Value>(&{
		let mut vec = Vec::new();
		fs::File::open(path)
			.expect("Failed to open file")
			.read_to_end(&mut vec)
			.expect("Failed to read file");
		vec
	})
	.expect("Failed to open file as JSON");

	if val.get("entityTemplates").is_some() {
		convert_2016_factory_to_modern(&from_value(val).expect("Failed to read file as RT struct"))
	} else {
		from_value(val).expect("Failed to read file as RT struct")
	}
}

pub fn read_as_rtblueprint(path: &str) -> RTBlueprint {
	let val = from_slice::<Value>(&{
		let mut vec = Vec::new();
		fs::File::open(path)
			.expect("Failed to open file")
			.read_to_end(&mut vec)
			.expect("Failed to read file");
		vec
	})
	.expect("Failed to open file as JSON");

	if val.get("entityTemplates").is_some() {
		convert_2016_blueprint_to_modern(&from_value(val).expect("Failed to read file as RT struct"))
	} else {
		from_value(val).expect("Failed to read file as RT struct")
	}
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

pub fn to_vec_float_format<W>(contents: &W) -> Vec<u8>
where
	W: ?Sized + Serialize
{
	let mut writer = Vec::with_capacity(128);

	let mut ser = Serializer::with_formatter(&mut writer, FloatFormatter);
	contents.serialize(&mut ser).unwrap();

	writer
}

#[derive(Clone, Debug)]
struct FloatFormatter;

impl Formatter for FloatFormatter {
	#[inline]
	fn write_f32<W>(&mut self, writer: &mut W, value: f32) -> io::Result<()>
	where
		W: ?Sized + io::Write
	{
		writer.write_all(value.to_string().as_bytes())
	}

	#[inline]
	fn write_f64<W>(&mut self, writer: &mut W, value: f64) -> io::Result<()>
	where
		W: ?Sized + io::Write
	{
		writer.write_all(value.to_string().as_bytes())
	}

	/// Writes a number that has already been rendered to a string.
	#[inline]
	fn write_number_str<W>(&mut self, writer: &mut W, value: &str) -> io::Result<()>
	where
		W: ?Sized + io::Write
	{
		let x = value.parse::<f64>();
		if let Ok(y) = x {
			if value.parse::<u64>().is_err() || y.to_string() == value.parse::<u64>().unwrap().to_string() {
				writer
					.write_all(
						if y.to_string() == "-0" {
							"0".to_string()
						} else {
							y.to_string()
						}
						.as_bytes()
					)
					.unwrap();
			} else {
				writer.write_all(value.as_bytes()).unwrap();
			}
		} else {
			writer.write_all(value.as_bytes()).unwrap();
		}

		Ok(())
	}
}
