use std::fs;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use hitman_commons::{
	metadata::ResourceMetadata,
	resourcelib::{EntityBlueprint, EntityFactory},
	rpkg_tool::RpkgResourceMeta
};
use serde_json::from_slice;

fn criterion_benchmark(c: &mut Criterion) {
	let mut group = c.benchmark_group("corpus");

	let corpus_folders = fs::read_dir("corpus").unwrap().flatten().collect::<Vec<_>>();

	for item in corpus_folders {
		let temp_path = fs::read_dir(item.path())
			.unwrap()
			.flatten()
			.find(|x| x.path().to_string_lossy().to_lowercase().ends_with(".temp.json"))
			.unwrap()
			.path();

		let temp_meta_path = fs::read_dir(item.path())
			.unwrap()
			.flatten()
			.find(|x| x.path().to_string_lossy().to_lowercase().ends_with(".temp.meta.json"))
			.unwrap()
			.path();

		let tblu_path = fs::read_dir(item.path())
			.unwrap()
			.flatten()
			.find(|x| x.path().to_string_lossy().to_lowercase().ends_with(".tblu.json"))
			.unwrap()
			.path();

		let tblu_meta_path = fs::read_dir(item.path())
			.unwrap()
			.flatten()
			.find(|x| x.path().to_string_lossy().to_lowercase().ends_with(".tblu.meta.json"))
			.unwrap()
			.path();

		let fac = from_slice::<EntityFactory>(&fs::read(temp_path).unwrap()).unwrap();

		let fac_meta =
			ResourceMetadata::try_from(from_slice::<RpkgResourceMeta>(&fs::read(temp_meta_path).unwrap()).unwrap())
				.unwrap();

		let blu = from_slice::<EntityBlueprint>(&fs::read(tblu_path).unwrap()).unwrap();

		let blu_meta =
			ResourceMetadata::try_from(from_slice::<RpkgResourceMeta>(&fs::read(tblu_meta_path).unwrap()).unwrap())
				.unwrap();

		group.bench_function(format!("{} -- convert", item.file_name().to_string_lossy()), |b| {
			b.iter(|| {
				quickentity_rs::convert_to_qn(
					black_box(&fac),
					black_box(&fac_meta),
					black_box(&blu),
					black_box(&blu_meta),
					black_box(false)
				)
			})
		});

		let converted = quickentity_rs::convert_to_qn(
			black_box(&fac),
			black_box(&fac_meta),
			black_box(&blu),
			black_box(&blu_meta),
			black_box(false)
		)
		.unwrap();

		group.bench_function(format!("{} -- generate", item.file_name().to_string_lossy()), |b| {
			b.iter(|| quickentity_rs::convert_to_rl(black_box(&converted)))
		});
	}

	group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
