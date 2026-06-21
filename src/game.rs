use glacier_commons::metadata::{RuntimeID, ResourceMetadata};

use crate::{HashMap, entity::EntityID};

pub trait ToQuickEntity {
	type QuickEntity;
	type Error;

	type Factory;
	type Blueprint;

	fn to_qn(
		&self,
		factory: &Self::Factory,
		factory_meta: &ResourceMetadata,
		blueprint: &Self::Blueprint,
		blueprint_meta: &ResourceMetadata,
		lossless: bool
	) -> Result<Self::QuickEntity, Self::Error>;
}

pub trait FromQuickEntity<T>: Sized {
	type Error;

	fn from_qn(
		input: &T,
		entity_indices: &HashMap<EntityID, usize>,
		reference_indices: &HashMap<RuntimeID, usize>,
		external_scene_indices: &HashMap<RuntimeID, usize>
	) -> Result<Self, Self::Error>;
}

pub trait ToGame<T> {
	type Error;

	fn to_game(
		&self,
		entity_indices: &HashMap<EntityID, usize>,
		reference_indices: &HashMap<RuntimeID, usize>,
		external_scene_indices: &HashMap<RuntimeID, usize>
	) -> Result<T, Self::Error>;
}

impl<T: FromQuickEntity<U>, U> ToGame<T> for U {
	type Error = T::Error;

	fn to_game(
		&self,
		entity_indices: &HashMap<EntityID, usize>,
		reference_indices: &HashMap<RuntimeID, usize>,
		external_scene_indices: &HashMap<RuntimeID, usize>
	) -> Result<T, Self::Error> {
		T::from_qn(self, entity_indices, reference_indices, external_scene_indices)
	}
}

pub(crate) mod types {
	#[cfg(feature = "h1")]
	pub mod h1 {
		pub type Factory = glacier_bin1::game::h1::STemplateEntity;
		pub type Blueprint = glacier_bin1::game::h1::STemplateEntityBlueprint;
	}

	#[cfg(feature = "h2")]
	pub mod h2 {
		pub type Factory = glacier_bin1::game::h2::STemplateEntityFactory;
		pub type Blueprint = glacier_bin1::game::h2::STemplateEntityBlueprint;
	}

	#[cfg(feature = "h3")]
	pub mod h3 {
		pub type Factory = glacier_bin1::game::h3::STemplateEntityFactory;
		pub type Blueprint = glacier_bin1::game::h3::STemplateEntityBlueprint;
	}

	#[cfg(feature = "fl")]
	pub mod fl {
		pub type Factory = glacier_bin1::game::fl::STemplateEntityFactory;
		pub type Blueprint = glacier_bin1::game::fl::STemplateEntityBlueprint;
	}
}
