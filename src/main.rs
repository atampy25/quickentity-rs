mod io_utils;

use std::fs;

use quickentity_rs::{apply_patch, entity::Entity, generate_patch, patch::Patch};

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::from_slice;
use tryvial::try_fn;

use crate::io_utils::{read_as_json, to_vec_float_format};

#[derive(Parser)]
#[command(author = "Atampy26", version, about = "A tool for parsing ResourceTool/RPKG entity JSON files into a more readable format and back again.", long_about = None)]
struct Args {
	#[command(subcommand)]
	command: Command
}

#[derive(ValueEnum, Clone, Copy)]
enum GameVersion {
	#[cfg(feature = "h1")]
	H1,

	#[cfg(feature = "h2")]
	H2,

	#[cfg(feature = "h3")]
	H3,

	#[cfg(feature = "fl")]
	FL
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
	},

	/// Convert an RPKG tool meta.JSON file to a hitman-commons metadata.json file.
	ConvertMeta {
		/// Input RPKG tool meta.JSON path.
		input: String,

		/// Output hitman-commons metadata.json path.
		output: String
	}
}

#[derive(Subcommand)]
enum EntityCommand {
	/// Convert a set of JSON files into a QuickEntity JSON file.
	Convert {
		/// Game version.
		#[arg(short = 'v', long)]
		version: GameVersion,

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

		/// Game version.
		#[arg(short = 'v', long)]
		version: GameVersion,

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
		output_blueprint_meta: String
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

		/// Mitigate a serde-json issue where numbers are sometimes not considered equal by parsing JSON files twice.
		#[arg(long, action)]
		format_fix: bool
	}
}

#[try_fn]
#[hotpath::main]
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
					lossless,
					version
				}
		} => {
			let entity = match version {
				#[cfg(feature = "h1")]
				GameVersion::H1 => Entity::from_game(
					&read_as_json::<glacier_bin1::game::h1::STemplateEntity>(input_factory),
					&read_as_json(input_factory_meta),
					&read_as_json(input_blueprint),
					&read_as_json(input_blueprint_meta),
					lossless
				)?,

				#[cfg(feature = "h2")]
				GameVersion::H2 => Entity::from_game(
					&read_as_json::<glacier_bin1::game::h2::STemplateEntityFactory>(input_factory),
					&read_as_json(input_factory_meta),
					&read_as_json(input_blueprint),
					&read_as_json(input_blueprint_meta),
					lossless
				)?,

				#[cfg(feature = "h3")]
				GameVersion::H3 => Entity::from_game(
					&read_as_json::<glacier_bin1::game::h3::STemplateEntityFactory>(input_factory),
					&read_as_json(input_factory_meta),
					&read_as_json(input_blueprint),
					&read_as_json(input_blueprint_meta),
					lossless
				)?,

				#[cfg(feature = "fl")]
				GameVersion::FL => Entity::from_game(
					&read_as_json::<glacier_bin1::game::fl::STemplateEntityFactory>(input_factory),
					&read_as_json(input_factory_meta),
					&read_as_json(input_blueprint),
					&read_as_json(input_blueprint_meta),
					lossless
				)?
			};

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
					version
				}
		} => match version {
			#[cfg(feature = "h1")]
			GameVersion::H1 => {
				let (converted_fac, converted_fac_meta, converted_blu, converted_blu_meta) =
					read_as_json::<Entity>(input).to_game::<(glacier_bin1::game::h1::STemplateEntity, _, _, _)>()?;

				fs::write(output_factory, to_vec_float_format(&converted_fac)).unwrap();
				fs::write(output_factory_meta, to_vec_float_format(&converted_fac_meta)).unwrap();
				fs::write(output_blueprint, to_vec_float_format(&converted_blu)).unwrap();
				fs::write(output_blueprint_meta, to_vec_float_format(&converted_blu_meta)).unwrap();
			}

			#[cfg(feature = "h2")]
			GameVersion::H2 => {
				let (converted_fac, converted_fac_meta, converted_blu, converted_blu_meta) =
					read_as_json::<Entity>(input)
						.to_game::<(glacier_bin1::game::h2::STemplateEntityFactory, _, _, _)>()?;

				fs::write(output_factory, to_vec_float_format(&converted_fac)).unwrap();
				fs::write(output_factory_meta, to_vec_float_format(&converted_fac_meta)).unwrap();
				fs::write(output_blueprint, to_vec_float_format(&converted_blu)).unwrap();
				fs::write(output_blueprint_meta, to_vec_float_format(&converted_blu_meta)).unwrap();
			}

			#[cfg(feature = "h3")]
			GameVersion::H3 => {
				let (converted_fac, converted_fac_meta, converted_blu, converted_blu_meta) =
					read_as_json::<Entity>(input)
						.to_game::<(glacier_bin1::game::h3::STemplateEntityFactory, _, _, _)>()?;

				fs::write(output_factory, to_vec_float_format(&converted_fac)).unwrap();
				fs::write(output_factory_meta, to_vec_float_format(&converted_fac_meta)).unwrap();
				fs::write(output_blueprint, to_vec_float_format(&converted_blu)).unwrap();
				fs::write(output_blueprint_meta, to_vec_float_format(&converted_blu_meta)).unwrap();
			}

			#[cfg(feature = "fl")]
			GameVersion::FL => {
				let (converted_fac, converted_fac_meta, converted_blu, converted_blu_meta) =
					read_as_json::<Entity>(input)
						.to_game::<(glacier_bin1::game::fl::STemplateEntityFactory, _, _, _)>()?;

				fs::write(output_factory, to_vec_float_format(&converted_fac)).unwrap();
				fs::write(output_factory_meta, to_vec_float_format(&converted_fac_meta)).unwrap();
				fs::write(output_blueprint, to_vec_float_format(&converted_blu)).unwrap();
				fs::write(output_blueprint_meta, to_vec_float_format(&converted_blu_meta)).unwrap();
			}
		},

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
			subcommand: PatchCommand::Apply {
				input,
				patch,
				output,
				permissive,
				format_fix
			}
		} => {
			let mut entity: Entity = read_as_json(input);
			let mut patch: Patch = read_as_json(patch);

			if format_fix {
				entity = from_slice(&to_vec_float_format(&entity))?;
				patch = from_slice(&to_vec_float_format(&patch))?;
			}

			let mut diagnostics_result = None;

			apply_patch(&mut entity, patch, |diagnostic| {
				if permissive {
					log::warn!("QuickEntity warning: {diagnostic}");
				} else {
					diagnostics_result.get_or_insert(diagnostic);
				}
			})?;

			diagnostics_result.map_or(Ok(()), |e| Err(e))?;

			fs::write(output, to_vec_float_format(&entity)).unwrap();
		}

		Command::ConvertMeta { input, output } => {
			let meta = glacier_commons::metadata::ResourceMetadata::try_from(read_as_json::<
				glacier_commons::rpkg_tool::RpkgResourceMeta
			>(input))?;
			fs::write(output, to_vec_float_format(&meta)).unwrap();
		}
	}
}
