mod io_utils;

use clap::{Parser, Subcommand};
use std::fs;

use quickentity_rs::{
	apply_patch, convert_modern_blueprint_to_2016, convert_modern_factory_to_2016, convert_to_qn,
	convert_to_rt, generate_patch,
};

use io_utils::*;

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
		#[clap(short = 'i', long)]
		input_factory: String,

		/// Factory (TEMP) meta JSON path.
		#[clap(short = 'j', long)]
		input_factory_meta: String,

		/// Blueprint (TBLU) JSON path.
		#[clap(short = 'k', long)]
		input_blueprint: String,

		/// Blueprint (TBLU) meta JSON path.
		#[clap(short = 'l', long)]
		input_blueprint_meta: String,

		/// Output QuickEntity JSON path.
		#[clap(short = 'o', long)]
		output: String,

		/// Convert keeping all scale values, no matter if insignificant (1.00 when rounded to 2 d.p.).
		#[clap(short = 's', long, action)]
		lossless: bool,

		/// Display performance data once finished.
		#[clap(long, action)]
		profile: bool,
	},

	/// Generate a set of JSON files from a QuickEntity JSON file.
	Generate {
		/// Input QuickEntity JSON path.
		#[clap(short = 'i', long)]
		input: String,

		/// Factory (TEMP) JSON path.
		#[clap(short = 'o', long)]
		output_factory: String,

		/// Factory (TEMP) meta JSON path.
		#[clap(short = 'p', long)]
		output_factory_meta: String,

		/// Blueprint (TBLU) JSON path.
		#[clap(short = 'q', long)]
		output_blueprint: String,

		/// Blueprint (TBLU) meta JSON path.
		#[clap(short = 'r', long)]
		output_blueprint_meta: String,

		/// Display performance data once finished.
		#[clap(long, action)]
		profile: bool,

		/// Output RT JSON files compatible with HITMAN (2016).
		#[clap(long, action)]
		h1: bool,
	},
}

#[derive(Subcommand)]
enum PatchCommand {
	/// Generate a patch JSON that transforms one entity JSON file into another.
	Generate {
		/// Original QuickEntity JSON path.
		#[clap(short = 'i', long)]
		input1: String,

		/// Modified QuickEntity JSON path.
		#[clap(short = 'j', long)]
		input2: String,

		/// Output patch JSON path.
		#[clap(short = 'o', long)]
		output: String,

		/// Display performance data once finished.
		#[clap(long, action)]
		profile: bool,
	},

	/// Apply a patch JSON to an entity JSON file.
	Apply {
		/// QuickEntity JSON path.
		#[clap(short = 'i', long)]
		input: String,

		/// Patch JSON path.
		#[clap(short = 'j', long)]
		patch: String,

		/// Output QuickEntity JSON path.
		#[clap(short = 'o', long)]
		output: String,

		/// Display performance data once finished.
		#[clap(long, action)]
		profile: bool,
	},
}

fn main() {
	let args = Args::parse();

	match args.command {
		Command::Entity {
			subcommand:
				EntityCommand::Convert {
					input_factory,
					input_factory_meta,
					input_blueprint,
					input_blueprint_meta,
					output,
					lossless,
					profile,
				},
		} => {
			if profile {
				time_graph::enable_data_collection(true);
			}

			let factory = read_as_rtfactory(&input_factory);
			let factory_meta = read_as_meta(&input_factory_meta);
			let blueprint = read_as_rtblueprint(&input_blueprint);
			let blueprint_meta = read_as_meta(&input_blueprint_meta);

			let entity = convert_to_qn(
				&factory,
				&factory_meta,
				&blueprint,
				&blueprint_meta,
				lossless,
			);

			fs::write(output, to_vec_float_format(&entity)).unwrap();

			if profile {
				println!("{}", time_graph::get_full_graph().as_table());
			}
		}

		Command::Entity {
			subcommand:
				EntityCommand::Generate {
					input,
					output_factory,
					output_factory_meta,
					output_blueprint,
					output_blueprint_meta,
					profile,
					h1,
				},
		} => {
			if profile {
				time_graph::enable_data_collection(true);
			}

			let entity = read_as_entity(&input);

			let (converted_fac, converted_fac_meta, converted_blu, converted_blu_meta) =
				convert_to_rt(&entity);

			fs::write(&output_factory, {
				if h1 {
					to_vec_float_format(&convert_modern_factory_to_2016(&converted_fac))
				} else {
					to_vec_float_format(&converted_fac)
				}
			})
			.unwrap();

			fs::write(
				&output_factory_meta,
				to_vec_float_format(&converted_fac_meta),
			)
			.unwrap();

			fs::write(&output_blueprint, {
				if h1 {
					to_vec_float_format(&convert_modern_blueprint_to_2016(&converted_blu))
				} else {
					to_vec_float_format(&converted_blu)
				}
			})
			.unwrap();

			fs::write(
				&output_blueprint_meta,
				to_vec_float_format(&converted_blu_meta),
			)
			.unwrap();

			if profile {
				println!("{}", time_graph::get_full_graph().as_table());
			}
		}

		Command::Patch {
			subcommand:
				PatchCommand::Generate {
					input1,
					input2,
					output,
					profile,
				},
		} => {
			if profile {
				time_graph::enable_data_collection(true);
			}

			let entity1 = read_as_value(&input1);
			let entity2 = read_as_value(&input2);

			let patch = generate_patch(&entity1, &entity2);

			fs::write(&output, to_vec_float_format(&patch)).unwrap();

			if profile {
				println!("{}", time_graph::get_full_graph().as_table());
			}
		}

		Command::Patch {
			subcommand: PatchCommand::Apply {
				input,
				patch,
				output,
				profile,
			},
		} => {
			if profile {
				time_graph::enable_data_collection(true);
			}

			let mut entity = read_as_value(&input);
			let patch = read_as_value(&patch);

			apply_patch(&mut entity, &patch);

			fs::write(&output, to_vec_float_format(&entity)).unwrap();

			if profile {
				println!("{}", time_graph::get_full_graph().as_table());
			}
		}
	}
}
