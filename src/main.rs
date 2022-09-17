mod qn_structs;
mod quickentity;
mod rpkg_structs;
mod rt_structs;
mod util_structs;

use rpkg_structs::ResourceMeta;
use rt_structs::{RTBlueprint, RTFactory};

use serde::Serialize;
use serde_json::ser::Formatter;
use serde_json::{from_slice, Serializer};
use std::io;
use std::time::{Instant, SystemTime};
use std::{fs, io::Read};

use crate::quickentity::{convert_to_qn, convert_to_rt};

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
		&fs::read_dir("corpus\\miami")
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
			.to_string()
	);
	let fac_meta = read_as_meta(&String::from(
		&fs::read_dir("corpus\\miami")
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
			.to_string()
	));
	let blu = read_as_rtblueprint(
		&fs::read_dir("corpus\\miami")
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
			.to_string()
	);
	let blu_meta = read_as_meta(
		&fs::read_dir("corpus\\miami")
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
			.to_string()
	);

	let entity = timeit(|| convert_to_qn(&fac, &fac_meta, &blu, &blu_meta));

	// dbg!(&entity);

	fs::write("entity.json", to_vec_float_format(&entity)).unwrap();

	let (converted_fac, converted_fac_meta, converted_blu, converted_blu_meta) =
		timeit(|| convert_to_rt(&entity));

	fs::write(
		"outputs\\miami\\factory.json",
		to_vec_float_format(&converted_fac)
	)
	.unwrap();

	fs::write(
		"outputs\\miami\\factory.meta.json",
		to_vec_float_format(&converted_fac_meta)
	)
	.unwrap();

	fs::write(
		"outputs\\miami\\blueprint.json",
		to_vec_float_format(&converted_blu)
	)
	.unwrap();

	fs::write(
		"outputs\\miami\\blueprint.meta.json",
		to_vec_float_format(&converted_blu_meta)
	)
	.unwrap();

	let elapsed = now.elapsed();
	println!("Elapsed: {:.2?}", elapsed);
}

fn to_vec_float_format<W>(contents: &W) -> Vec<u8>
where
	W: ?Sized + Serialize
{
	let mut writer = Vec::with_capacity(128);

	let mut ser = Serializer::with_formatter(&mut writer, FloatFormatter);
	contents.serialize(&mut ser).unwrap();

	writer
}

#[derive(Clone, Debug)]
pub struct FloatFormatter;

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
			if value.parse::<u64>().is_err()
				|| y.to_string() == value.parse::<u64>().unwrap().to_string()
			{
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

fn timeit<F: FnMut() -> T, T>(mut f: F) -> T {
	let start = SystemTime::now();
	let result = f();
	let end = SystemTime::now();
	let duration = end.duration_since(start).unwrap();
	println!("Fn took {} milliseconds", duration.as_millis());
	result
}
