mod io_utils;

use std::{fs, io::Read};

use criterion::{criterion_group, criterion_main, Criterion};

use io_utils::*;
use quickentity_rs::{
	apply_patch, convert_to_qn, convert_to_rt, generate_patch,
	qn_structs::Entity,
	rpkg_structs::ResourceMeta,
	rt_structs::{RTBlueprint, RTFactory}
};
use serde_json::{from_slice, Value};

fn benchmark(c: &mut Criterion) {
	let factory_ic = read_as_rtfactory("corpus\\items colombia\\0086A9F758D6E876.TEMP.json");
	let factory_meta_ic = read_as_meta("corpus\\items colombia\\0086A9F758D6E876.TEMP.meta.JSON");
	let blueprint_ic = read_as_rtblueprint("corpus\\items colombia\\00E0C6BE6AB47041.TBLU.json");
	let blueprint_meta_ic = read_as_meta("corpus\\items colombia\\00E0C6BE6AB47041.TBLU.meta.JSON");

	let factory_sm = read_as_rtfactory("corpus\\miami\\0056406088754691.TEMP.json");
	let factory_meta_sm = read_as_meta("corpus\\miami\\0056406088754691.TEMP.meta.JSON");
	let blueprint_sm = read_as_rtblueprint("corpus\\miami\\00CF14C55C3BCCA8.TBLU.json");
	let blueprint_meta_sm = read_as_meta("corpus\\miami\\00CF14C55C3BCCA8.TBLU.meta.JSON");

	let factory_gp = read_as_rtfactory("corpus\\goty paris\\00CFDE2BFD950D98.TEMP.json");
	let factory_meta_gp = read_as_meta("corpus\\goty paris\\00CFDE2BFD950D98.TEMP.meta.JSON");
	let blueprint_gp = read_as_rtblueprint("corpus\\goty paris\\00A4F3F95D1DC1A4.TBLU.json");
	let blueprint_meta_gp = read_as_meta("corpus\\goty paris\\00A4F3F95D1DC1A4.TBLU.meta.JSON");

	let factory_se = read_as_rtfactory("corpus\\scenario_eagle\\0041AF5ED74266C0.TEMP.json");
	let factory_meta_se = read_as_meta("corpus\\scenario_eagle\\0041AF5ED74266C0.TEMP.meta.JSON");
	let blueprint_se = read_as_rtblueprint("corpus\\scenario_eagle\\0080946D1FB8EFDB.TBLU.json");
	let blueprint_meta_se = read_as_meta("corpus\\scenario_eagle\\0080946D1FB8EFDB.TBLU.meta.JSON");

	c.bench_function("items colombia", |b| {
		b.iter(|| {
			convert_to_qn(
				&factory_ic,
				&factory_meta_ic,
				&blueprint_ic,
				&blueprint_meta_ic,
				true
			);
		})
	});

	c.bench_function("scenario_miami", |b| {
		b.iter(|| {
			convert_to_qn(
				&factory_sm,
				&factory_meta_sm,
				&blueprint_sm,
				&blueprint_meta_sm,
				true
			);
		})
	});

	c.bench_function("goty paris", |b| {
		b.iter(|| {
			convert_to_qn(
				&factory_gp,
				&factory_meta_gp,
				&blueprint_gp,
				&blueprint_meta_gp,
				true
			);
		})
	});

	c.bench_function("scenario_eagle", |b| {
		b.iter(|| {
			convert_to_qn(
				&factory_se,
				&factory_meta_se,
				&blueprint_se,
				&blueprint_meta_se,
				true
			);
		})
	});
}

criterion_group!(conversion, benchmark);
criterion_main!(conversion);
