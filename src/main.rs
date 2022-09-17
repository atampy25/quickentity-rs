mod qn_structs;
mod quickentity;
mod rpkg_structs;
mod rt_structs;
mod util_structs;

use qn_structs::Entity;
use rpkg_structs::ResourceMeta;
use rt_structs::{RTBlueprint, RTFactory};

use clap::{Parser, Subcommand};
use serde::Serialize;
use serde_json::ser::Formatter;
use serde_json::{from_slice, Serializer, Value};
use std::io;
use std::time::Instant;
use std::{fs, io::Read};

use crate::quickentity::{apply_patch, convert_to_qn, convert_to_rt, generate_patch};

fn read_as_value(path: &String) -> Value {
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

#[derive(Parser)]
#[clap(author = "Atampy26", version, about = "A tool for parsing ResourceTool/RPKG entity JSON files into a more readable format and back again.", long_about = None)]
struct Args {
	#[clap(subcommand)]
	command: Command,
}

#[derive(Subcommand)]
enum Command {
	// Convert between RT/RPKG source files and QuickEntity entity JSON files.
	Entity {
		#[clap(subcommand)]
		subcommand: EntityCommand,
	},

	// Generate or apply a QuickEntity patch JSON.
	Patch {
		#[clap(subcommand)]
		subcommand: PatchCommand,
	},
}

#[derive(Subcommand)]
enum EntityCommand {
	/// Convert a set of JSON files into a QuickEntity JSON file.
	Convert {
		/// Factory (TEMP) JSON path.
		#[clap(short, long)]
		input_factory: String,

		/// Factory (TEMP) meta JSON path.
		#[clap(short, long)]
		input_factory_meta: String,

		/// Blueprint (TBLU) JSON path.
		#[clap(short, long)]
		input_blueprint: String,

		/// Blueprint (TBLU) meta JSON path.
		#[clap(short, long)]
		input_blueprint_meta: String,

		/// Output QuickEntity JSON path.
		#[clap(short, long)]
		output: String,
	},

	/// Generate a set of JSON files from a QuickEntity JSON file.
	Generate {
		/// Input QuickEntity JSON path.
		#[clap(short, long)]
		input: String,

		/// Factory (TEMP) JSON path.
		#[clap(short, long)]
		output_factory: String,

		/// Factory (TEMP) meta JSON path.
		#[clap(short, long)]
		output_factory_meta: String,

		/// Blueprint (TBLU) JSON path.
		#[clap(short, long)]
		output_blueprint: String,

		/// Blueprint (TBLU) meta JSON path.
		#[clap(short, long)]
		output_blueprint_meta: String,
	},
}

#[derive(Subcommand)]
enum PatchCommand {
	/// Generate a patch JSON that transforms one entity JSON file into another.
	Generate {
		/// Original QuickEntity JSON path.
		#[clap(short, long)]
		input1: String,

		/// Modified QuickEntity JSON path.
		#[clap(short, long)]
		input2: String,

		/// Output patch JSON path.
		#[clap(short, long)]
		output: String,
	},

	/// Apply a patch JSON to an entity JSON file.
	Apply {
		/// QuickEntity JSON path.
		#[clap(short, long)]
		input: String,

		/// Patch JSON path.
		#[clap(short, long)]
		patch: String,

		/// Output QuickEntity JSON path.
		#[clap(short, long)]
		output: String,
	},
}

fn main() {
	let args = Args::parse();

	let now = Instant::now();

	match args.command {
		Command::Entity {
			subcommand:
				EntityCommand::Convert {
					input_factory,
					input_factory_meta,
					input_blueprint,
					input_blueprint_meta,
					output,
				},
		} => {
			let factory = read_as_rtfactory(&input_factory);
			let factory_meta = read_as_meta(&input_factory_meta);
			let blueprint = read_as_rtblueprint(&input_blueprint);
			let blueprint_meta = read_as_meta(&input_blueprint_meta);

			let entity = convert_to_qn(&factory, &factory_meta, &blueprint, &blueprint_meta);

			fs::write(output, to_vec_float_format(&entity)).unwrap();
		}

		Command::Entity {
			subcommand:
				EntityCommand::Generate {
					input,
					output_factory,
					output_factory_meta,
					output_blueprint,
					output_blueprint_meta,
				},
		} => {
			let entity = read_as_entity(&input);

			let (converted_fac, converted_fac_meta, converted_blu, converted_blu_meta) =
				convert_to_rt(&entity);

			fs::write(&output_factory, to_vec_float_format(&converted_fac)).unwrap();

			fs::write(
				&output_factory_meta,
				to_vec_float_format(&converted_fac_meta),
			)
			.unwrap();

			fs::write(&output_blueprint, to_vec_float_format(&converted_blu)).unwrap();

			fs::write(
				&output_blueprint_meta,
				to_vec_float_format(&converted_blu_meta),
			)
			.unwrap();
		}

		Command::Patch {
			subcommand: PatchCommand::Generate {
				input1,
				input2,
				output,
			},
		} => {
			let entity1 = read_as_value(&input1);
			let entity2 = read_as_value(&input2);

			let patch = generate_patch(&entity1, &entity2);

			fs::write(&output, to_vec_float_format(&patch)).unwrap();
		}

		Command::Patch {
			subcommand: PatchCommand::Apply {
				input,
				patch,
				output,
			},
		} => {
			let mut entity = read_as_value(&input);
			let patch = read_as_value(&patch);

			apply_patch(&mut entity, &patch);

			fs::write(&output, to_vec_float_format(&entity)).unwrap();
		}
	}

	let elapsed = now.elapsed();
	println!("Elapsed: {:.2?}", elapsed);
}

fn to_vec_float_format<W>(contents: &W) -> Vec<u8>
where
	W: ?Sized + Serialize,
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
		W: ?Sized + io::Write,
	{
		writer.write_all(value.to_string().as_bytes())
	}

	#[inline]
	fn write_f64<W>(&mut self, writer: &mut W, value: f64) -> io::Result<()>
	where
		W: ?Sized + io::Write,
	{
		writer.write_all(value.to_string().as_bytes())
	}

	/// Writes a number that has already been rendered to a string.
	#[inline]
	fn write_number_str<W>(&mut self, writer: &mut W, value: &str) -> io::Result<()>
	where
		W: ?Sized + io::Write,
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
						.as_bytes(),
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
