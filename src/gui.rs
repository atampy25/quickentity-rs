#![warn(clippy::all, rust_2018_idioms)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod io_utils;

use std::fs;

use egui::{CentralPanel, Context, RichText};
use quickentity_rs::{apply_patch, convert_to_qn, convert_to_rt, generate_patch};
use rfd::FileDialog;

use io_utils::*;

fn main() {
	eframe::run_native(
		"QuickEntity GUI",
		eframe::NativeOptions::default(),
		Box::new(|cc| Box::new(App::new(cc)))
	);
}
pub struct App {}

impl App {
	pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
		App {}
	}
}

impl eframe::App for App {
	fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
		ctx.set_pixels_per_point(3.0);

		CentralPanel::default().show(ctx, |ui| {
			ui.label(RichText::from("QuickEntity").strong());

			ui.label("Entities");

			if ui
				.button(RichText::from("Convert RT source files to QuickEntity JSON").size(8.0))
				.clicked()
			{
				std::thread::spawn(move || {
					let input_factory = FileDialog::new()
						.add_filter("TEMP.json files", &["TEMP.json"])
						.pick_file()
						.unwrap()
						.to_str()
						.unwrap()
						.to_owned();

					let input_factory_meta = FileDialog::new()
						.add_filter("TEMP.meta.JSON files", &["TEMP.meta.JSON"])
						.pick_file()
						.unwrap()
						.to_str()
						.unwrap()
						.to_owned();

					let input_blueprint = FileDialog::new()
						.add_filter("TBLU.json files", &["TBLU.json"])
						.pick_file()
						.unwrap()
						.to_str()
						.unwrap()
						.to_owned();

					let input_blueprint_meta = FileDialog::new()
						.add_filter("TBLU.meta.JSON files", &["TBLU.meta.JSON"])
						.pick_file()
						.unwrap()
						.to_str()
						.unwrap()
						.to_owned();

					let output = FileDialog::new()
						.add_filter("entity.json files", &["entity.json"])
						.save_file()
						.unwrap();

					let factory = read_as_rtfactory(&input_factory);
					let factory_meta = read_as_meta(&input_factory_meta);
					let blueprint = read_as_rtblueprint(&input_blueprint);
					let blueprint_meta = read_as_meta(&input_blueprint_meta);

					let entity =
						convert_to_qn(&factory, &factory_meta, &blueprint, &blueprint_meta, false);

					fs::write(output, to_vec_float_format(&entity)).unwrap();
				});
			}

			if ui
				.button(RichText::from("Convert QuickEntity JSON to RT source files").size(8.0))
				.clicked()
			{
				std::thread::spawn(move || {
					let input = FileDialog::new()
						.add_filter("entity.json files", &["entity.json"])
						.pick_file()
						.unwrap()
						.to_str()
						.unwrap()
						.to_owned();

					let output_factory = FileDialog::new()
						.add_filter("TEMP.json files", &["TEMP.json"])
						.save_file()
						.unwrap();

					let output_factory_meta = FileDialog::new()
						.add_filter("TEMP.meta.JSON files", &["TEMP.meta.JSON"])
						.save_file()
						.unwrap();

					let output_blueprint = FileDialog::new()
						.add_filter("TBLU.json files", &["TBLU.json"])
						.save_file()
						.unwrap();

					let output_blueprint_meta = FileDialog::new()
						.add_filter("TBLU.meta.JSON files", &["TBLU.meta.JSON"])
						.save_file()
						.unwrap();

					let entity = read_as_entity(&input);

					let (converted_fac, converted_fac_meta, converted_blu, converted_blu_meta) =
						convert_to_rt(&entity);

					fs::write(&output_factory, to_vec_float_format(&converted_fac)).unwrap();

					fs::write(
						&output_factory_meta,
						to_vec_float_format(&converted_fac_meta)
					)
					.unwrap();

					fs::write(&output_blueprint, to_vec_float_format(&converted_blu)).unwrap();

					fs::write(
						&output_blueprint_meta,
						to_vec_float_format(&converted_blu_meta)
					)
					.unwrap();
				});
			}

			ui.label("Patches");

			if ui
				.button(RichText::from("Generate patch from QuickEntity JSONs").size(8.0))
				.clicked()
			{
				std::thread::spawn(move || {
					let input1 = FileDialog::new()
						.set_title("Select the original entity JSON")
						.add_filter("entity.json files", &["entity.json"])
						.pick_file()
						.unwrap()
						.to_str()
						.unwrap()
						.to_owned();

					let input2 = FileDialog::new()
						.set_title("Select the modified entity JSON")
						.add_filter("entity.json files", &["entity.json"])
						.pick_file()
						.unwrap()
						.to_str()
						.unwrap()
						.to_owned();

					let output = FileDialog::new()
						.add_filter("entity.patch.json files", &["entity.patch.json"])
						.save_file()
						.unwrap();

					let entity1 = read_as_entity(&input1);
					let entity2 = read_as_entity(&input2);

					let patch = generate_patch(&entity1, &entity2);

					fs::write(&output, to_vec_float_format(&patch)).unwrap();
				});
			}

			if ui
				.button(RichText::from("Apply patch to QuickEntity JSON").size(8.0))
				.clicked()
			{
				std::thread::spawn(move || {
					let input = FileDialog::new()
						.set_title("Select the entity JSON")
						.add_filter("entity.json files", &["entity.json"])
						.pick_file()
						.unwrap()
						.to_str()
						.unwrap()
						.to_owned();

					let patch = FileDialog::new()
						.set_title("Select the patch JSON")
						.add_filter("entity.patch.json files", &["entity.patch.json"])
						.pick_file()
						.unwrap()
						.to_str()
						.unwrap()
						.to_owned();

					let output = FileDialog::new()
						.add_filter("entity.json files", &["entity.json"])
						.save_file()
						.unwrap();

					let mut entity = read_as_entity(&input);
					let patch = read_as_value(&patch);

					apply_patch(&mut entity, &patch, false);

					fs::write(&output, to_vec_float_format(&entity)).unwrap();
				});
			}
		});
	}
}
