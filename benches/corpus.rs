use std::{fs, hint::black_box};

use criterion::{Criterion, criterion_group, criterion_main};
use serde_json::from_slice;

#[cfg(feature = "h3")]
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

		let fac: glacier_bin1::game::h3::STemplateEntityFactory = from_slice(&fs::read(temp_path).unwrap()).unwrap();
		let fac_meta = from_slice(&fs::read(temp_meta_path).unwrap()).unwrap();
		let blu = from_slice(&fs::read(tblu_path).unwrap()).unwrap();
		let blu_meta = from_slice(&fs::read(tblu_meta_path).unwrap()).unwrap();

		group.bench_function(format!("{} -- convert", item.file_name().to_string_lossy()), |b| {
			b.iter(|| {
				quickentity_rs::entity::Entity::from_game(
					black_box(&fac),
					black_box(&fac_meta),
					black_box(&blu),
					black_box(&blu_meta),
					black_box(false)
				)
			})
		});

		let converted = quickentity_rs::entity::Entity::from_game(
			black_box(&fac),
			black_box(&fac_meta),
			black_box(&blu),
			black_box(&blu_meta),
			black_box(false)
		)
		.unwrap();

		group.bench_function(format!("{} -- generate", item.file_name().to_string_lossy()), |b| {
			b.iter(|| black_box(&converted).to_game::<(glacier_bin1::game::h3::STemplateEntityFactory, _, _, _)>())
		});
	}

	group.finish();
}

cfg_select! {
	feature = "h3" => {
		criterion_group!(benches, criterion_benchmark);
		criterion_main!(benches);
	}

	_ => { fn main() {} }
}
