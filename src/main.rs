mod io_utils;

use std::fs;

use quickentity_rs::{apply_patch, convert_to_game, convert_to_qn, entity::Entity, generate_patch, patch::Patch};

use anyhow::Result;
use clap::{Parser, Subcommand};
use hitman_commons::game::GameVersion;
use serde_json::from_slice;
use tryvial::try_fn;

use crate::io_utils::{read_as_json, to_vec_float_format};

#[derive(Parser)]
#[command(author = "Atampy26", version, about = "A tool for parsing ResourceTool/RPKG entity JSON files into a more readable format and back again.", long_about = None)]
struct Args {
	#[command(subcommand)]
	command: Command
}

#[derive(Subcommand)]
enum Command {
	/// Convert between ResourceLib/hitman-commons source files and QuickEntity entity JSON files.
	Entity {
		#[command(subcommand)]
		subcommand: EntityCommand
	},

	/// Generate or apply a QuickEntity patch JSON.
	Patch {
		#[command(subcommand)]
		subcommand: PatchCommand
	}
}

#[derive(Subcommand)]
enum EntityCommand {
	/// Convert a set of JSON files into a QuickEntity JSON file.
	Convert {
		/// Factory (TEMP) JSON path.
		#[arg(short = 'i', long)]
		input_factory: String,

		/// Factory (TEMP) meta JSON path.
		#[arg(short = 'j', long)]
		input_factory_meta: String,

		/// Blueprint (TBLU) JSON path.
		#[arg(short = 'k', long)]
		input_blueprint: String,

		/// Blueprint (TBLU) meta JSON path.
		#[arg(short = 'l', long)]
		input_blueprint_meta: String,

		/// Output QuickEntity JSON path.
		#[arg(short = 'o', long)]
		output: String,

		/// Convert keeping all scale values, no matter if insignificant (1.00 when rounded to 2 d.p.).
		#[arg(short = 's', long, action)]
		lossless: bool
	},

	/// Generate a set of JSON files from a QuickEntity JSON file.
	Generate {
		/// Input QuickEntity JSON path.
		#[arg(short = 'i', long)]
		input: String,

		/// Factory (TEMP) JSON path.
		#[arg(short = 'o', long)]
		output_factory: String,

		/// Factory (TEMP) meta JSON path.
		#[arg(short = 'p', long)]
		output_factory_meta: String,

		/// Blueprint (TBLU) JSON path.
		#[arg(short = 'q', long)]
		output_blueprint: String,

		/// Blueprint (TBLU) meta JSON path.
		#[arg(short = 'r', long)]
		output_blueprint_meta: String,

		/// Output ResourceLib JSON files compatible with HITMAN (2016).
		#[arg(long, action)]
		h1: bool
	},

	/// Output the same QuickEntity JSON in standard form, including consistent entity ID lengths and sorted JSON keys.
	Normalise {
		/// Input QuickEntity JSON path.
		#[arg(short = 'i', long)]
		input: String,

		/// Output QuickEntity JSON path.
		#[arg(short = 'o', long)]
		output: String,

		/// Convert keeping all scale values, no matter if insignificant (1.00 when rounded to 2 d.p.).
		#[arg(short = 's', long, action)]
		lossless: bool
	}
}

#[derive(Subcommand)]
enum PatchCommand {
	/// Generate a patch JSON that transforms one entity JSON file into another.
	Generate {
		/// Original QuickEntity JSON path.
		#[arg(short = 'i', long)]
		input1: String,

		/// Modified QuickEntity JSON path.
		#[arg(short = 'j', long)]
		input2: String,

		/// Output patch JSON path.
		#[arg(short = 'o', long)]
		output: String,

		/// Mitigate a serde-json issue where numbers are sometimes not considered equal by parsing JSON files twice.
		#[arg(long, action)]
		format_fix: bool
	},

	/// Apply a patch JSON to an entity JSON file.
	Apply {
		/// QuickEntity JSON path.
		#[arg(short = 'i', long)]
		input: String,

		/// Patch JSON path.
		#[arg(short = 'j', long)]
		patch: String,

		/// Output QuickEntity JSON path.
		#[arg(short = 'o', long)]
		output: String,

		/// Be more permissive with certain unexpected scenarios, such as properties that should be removed already being gone.
		#[arg(long, action)]
		permissive: bool,

		/// Ensure the resulting QuickEntity JSON is valid and output the JSON in standard form, including consistent entity ID lengths and sorted JSON keys.
		#[arg(long, action)]
		normalise: bool,

		/// Mitigate a serde-json issue where numbers are sometimes not considered equal by parsing JSON files twice.
		#[arg(long, action)]
		format_fix: bool
	}
}

#[try_fn]
fn main() -> Result<()> {
	if std::env::var("RUST_LOG").is_err() {
		unsafe { std::env::set_var("RUST_LOG", "info") }
	}

	env_logger::init();

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
					lossless
				}
		} => {
			let entity = convert_to_qn(
				&read_as_json(input_factory),
				&read_as_json(input_factory_meta),
				&read_as_json(input_blueprint),
				&read_as_json(input_blueprint_meta),
				lossless
			)?;

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
					h1
				}
		} => {
			let (converted_fac, converted_fac_meta, converted_blu, converted_blu_meta) =
				convert_to_game(&read_as_json(input), GameVersion::H3)?;

			fs::write(output_factory, {
				if h1 {
					// to_vec_float_format(&converted_fac.into_legacy())
					todo!()
				} else {
					to_vec_float_format(&converted_fac)
				}
			})
			.unwrap();

			fs::write(output_factory_meta, to_vec_float_format(&converted_fac_meta)).unwrap();

			fs::write(output_blueprint, {
				if h1 {
					// to_vec_float_format(&converted_blu.into_legacy())
					todo!()
				} else {
					to_vec_float_format(&converted_blu)
				}
			})
			.unwrap();

			fs::write(output_blueprint_meta, to_vec_float_format(&converted_blu_meta)).unwrap();
		}

		Command::Entity {
			subcommand: EntityCommand::Normalise {
				input,
				output,
				lossless
			}
		} => {
			let (factory, factory_meta, blueprint, blueprint_meta) =
				convert_to_game(&read_as_json(input), GameVersion::H3)?;
			let entity = convert_to_qn(&factory, &factory_meta, &blueprint, &blueprint_meta, lossless)?;

			fs::write(output, to_vec_float_format(&entity)).unwrap();
		}

		Command::Patch {
			subcommand: PatchCommand::Generate {
				input1,
				input2,
				output,
				format_fix
			}
		} => {
			let mut entity1: Entity = read_as_json(input1);
			let mut entity2: Entity = read_as_json(input2);

			if format_fix {
				entity1 = from_slice(&to_vec_float_format(&entity1))?;

				entity2 = from_slice(&to_vec_float_format(&entity2))?;
			}

			let patch = generate_patch(&entity1, &entity2)?;

			fs::write(output, to_vec_float_format(&patch)).unwrap();
		}

		Command::Patch {
			subcommand:
				PatchCommand::Apply {
					input,
					patch,
					output,
					permissive,
					normalise,
					format_fix
				}
		} => {
			let mut entity: Entity = read_as_json(input);
			let mut patch: Patch = read_as_json(patch);

			if format_fix {
				entity = from_slice(&to_vec_float_format(&entity))?;
				patch = from_slice(&to_vec_float_format(&patch))?;
			}

			if normalise {
				let (factory, factory_meta, blueprint, blueprint_meta) = convert_to_game(&entity, GameVersion::H3)?;
				entity = convert_to_qn(&factory, &factory_meta, &blueprint, &blueprint_meta, true)?;
			}

			apply_patch(&mut entity, patch, permissive)?;

			if normalise {
				let (factory, factory_meta, blueprint, blueprint_meta) = convert_to_game(&entity, GameVersion::H3)?;
				entity = convert_to_qn(&factory, &factory_meta, &blueprint, &blueprint_meta, true)?;
			}

			fs::write(output, to_vec_float_format(&entity)).unwrap();
		}
	}
}
