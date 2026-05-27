use std::{
	collections::HashMap,
	fmt::{self, Display, Formatter},
	str::FromStr
};

use anyhow::{Context, Result};
use ecow::{EcoString, eco_format};
use fn_error_context::context;
use glam::{Affine3, EulerRot, Mat3, Quat};
use hitman_bin1::{
	game::h3::{
		SColorRGB, SColorRGBA, SEntityTemplateReference, SMatrix43, STemplateEntityBlueprint, STemplateEntityFactory,
		SVector3, ZGuid, ZVariant
	},
	types::{repository::ZRepositoryID, resource::ZRuntimeResourceID}
};
use hitman_commons::{
	game::GameVersion,
	metadata::{ResourceMetadata, ResourceReference, RuntimeID}
};
use identity_hash::BuildIdentityHasher;
use serde::{
	Deserialize, Serialize,
	de::Error as _,
	ser::{Error as _, SerializeStruct}
};
use serde_json::{Value, from_value, json, to_value};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use specta::Type;
use tryvial::try_fn;
use uuid::Uuid;

use crate::entity::{EntityID, Ref};

#[cfg(feature = "rune")]
pub fn rune_module() -> Result<rune::Module, rune::ContextError> {
	let mut module = rune::Module::with_crate_item("quickentity_rs", ["variant"])?;

	module.ty::<Variant>()?;
	module.ty::<Transform>()?;
	module.ty::<Vec3>()?;
	module.ty::<ColorRGB>()?;
	module.ty::<ColorRGBA>()?;

	Ok(module)
}

#[cfg_attr(feature = "rune", serde_with::apply(_ => #[rune(get, set)]))]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::variant))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, CLONE))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
pub struct Transform {
	/// Position in 3D space.
	pub position: Vec3,

	/// Rotation in Euler XYZ angles (degrees).
	pub rotation: Vec3,

	#[serde(skip_serializing_if = "Option::is_none")]
	pub scale: Option<Vec3>
}

impl Transform {
	pub fn identity() -> Self {
		Self {
			position: Vec3 { x: 0.0, y: 0.0, z: 0.0 },
			rotation: Vec3 { x: 0.0, y: 0.0, z: 0.0 },
			scale: None
		}
	}

	pub fn from_glam(transform: Affine3, lossless: bool) -> Self {
		let (scale, rotation, translation) = transform.to_scale_rotation_translation();

		let scale_important = if lossless {
			// In lossless mode, preserve exact scale
			scale.x != 1.0 || scale.y != 1.0 || scale.z != 1.0
		} else {
			// Otherwise only emit if scale is not equal to 1.00 (to 2 d.p.)
			(scale.x * 100.0).round() != 100.0
				|| (scale.y * 100.0).round() != 100.0
				|| (scale.z * 100.0).round() != 100.0
		};

		if scale_important {
			Self {
				rotation: rotation.into(),
				position: translation.into(),
				scale: Some(scale.into())
			}
		} else {
			Self {
				rotation: rotation.into(),
				position: translation.into(),
				scale: None
			}
		}
	}

	pub fn from_game(transform: &SMatrix43, lossless: bool) -> Self {
		// Mat3 is column-major while SMatrix43 is row-major, so we have to transpose
		let transform = Affine3::from_mat3_translation(
			Mat3 {
				x_axis: glam::Vec3 {
					x: transform.x_axis.x,
					y: transform.y_axis.x,
					z: transform.z_axis.x
				},
				y_axis: glam::Vec3 {
					x: transform.x_axis.y,
					y: transform.y_axis.y,
					z: transform.z_axis.y
				},
				z_axis: glam::Vec3 {
					x: transform.x_axis.z,
					y: transform.y_axis.z,
					z: transform.z_axis.z
				}
			},
			glam::Vec3 {
				x: transform.trans.x,
				y: transform.trans.y,
				z: transform.trans.z
			}
		);

		Self::from_glam(transform, lossless)
	}

	pub fn to_glam(&self) -> Affine3 {
		let scale = if let Some(scale) = self.scale {
			scale.into()
		} else {
			glam::Vec3 { x: 1.0, y: 1.0, z: 1.0 }
		};

		Affine3::from_scale_rotation_translation(scale, self.rotation.into(), self.position.into())
	}

	pub fn to_game(&self) -> SMatrix43 {
		let transform = self.to_glam();

		// Transpose
		SMatrix43 {
			x_axis: SVector3 {
				x: transform.matrix3.x_axis.x,
				y: transform.matrix3.y_axis.x,
				z: transform.matrix3.z_axis.x
			},
			y_axis: SVector3 {
				x: transform.matrix3.x_axis.y,
				y: transform.matrix3.y_axis.y,
				z: transform.matrix3.z_axis.y
			},
			z_axis: SVector3 {
				x: transform.matrix3.x_axis.z,
				y: transform.matrix3.y_axis.z,
				z: transform.matrix3.z_axis.z
			},
			trans: SVector3 {
				x: transform.translation.x,
				y: transform.translation.y,
				z: transform.translation.z
			}
		}
	}
}

#[cfg_attr(feature = "rune", serde_with::apply(_ => #[rune(get, set)]))]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::variant))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, CLONE))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Type)]
pub struct Vec3 {
	pub x: f32,
	pub y: f32,
	pub z: f32
}

impl From<Vec3> for glam::Vec3 {
	fn from(value: Vec3) -> Self {
		glam::Vec3 {
			x: value.x,
			y: value.y,
			z: value.z
		}
	}
}

impl From<glam::Vec3> for Vec3 {
	fn from(value: glam::Vec3) -> Self {
		Vec3 {
			x: value.x,
			y: value.y,
			z: value.z
		}
	}
}

const RAD2DEG: f32 = 180.0 / std::f32::consts::PI;
const DEG2RAD: f32 = std::f32::consts::PI / 180.0;

impl From<Vec3> for Quat {
	fn from(value: Vec3) -> Self {
		Quat::from_euler(EulerRot::XYZ, value.x * DEG2RAD, value.y * DEG2RAD, value.z * DEG2RAD)
	}
}

impl From<Quat> for Vec3 {
	fn from(value: Quat) -> Self {
		let (x, y, z) = value.normalize().to_euler(EulerRot::XYZ);
		Vec3 {
			x: x * RAD2DEG,
			y: y * RAD2DEG,
			z: z * RAD2DEG
		}
	}
}

#[cfg_attr(feature = "rune", serde_with::apply(_ => #[rune(get, set)]))]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::variant))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, CLONE))]
#[derive(SerializeDisplay, DeserializeFromStr, Debug, Clone, PartialEq)]
pub struct ColorRGB {
	pub r: f32,
	pub g: f32,
	pub b: f32
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for ColorRGB {
	fn schema_name() -> std::borrow::Cow<'static, str> {
		"ColorRGB".into()
	}

	fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
		schemars::json_schema!({
			"type": "string",
			"pattern": "^#[0-9a-fA-F]{6}$"
		})
	}
}

impl Display for ColorRGB {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		write!(
			f,
			"#{:02x}{:02x}{:02x}",
			(self.r * 255.0).round() as u8,
			(self.g * 255.0).round() as u8,
			(self.b * 255.0).round() as u8
		)
	}
}

impl FromStr for ColorRGB {
	type Err = String;

	#[try_fn]
	fn from_str(s: &str) -> Result<Self, Self::Err> {
		if s.len() != 7 {
			return Err("Invalid color format".into());
		}

		let r = u8::from_str_radix(&s[1..3], 16).map_err(|e| e.to_string())?;
		let g = u8::from_str_radix(&s[3..5], 16).map_err(|e| e.to_string())?;
		let b = u8::from_str_radix(&s[5..7], 16).map_err(|e| e.to_string())?;

		Self {
			r: r as f32 / 255.0,
			g: g as f32 / 255.0,
			b: b as f32 / 255.0
		}
	}
}

#[cfg_attr(feature = "rune", serde_with::apply(_ => #[rune(get, set)]))]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::variant))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, CLONE))]
#[derive(SerializeDisplay, DeserializeFromStr, Debug, Clone, PartialEq)]
pub struct ColorRGBA {
	pub r: f32,
	pub g: f32,
	pub b: f32,
	pub a: f32
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for ColorRGBA {
	fn schema_name() -> std::borrow::Cow<'static, str> {
		"ColorRGBA".into()
	}

	fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
		schemars::json_schema!({
			"type": "string",
			"pattern": "^#[0-9a-fA-F]{8}$"
		})
	}
}

impl Display for ColorRGBA {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		write!(
			f,
			"#{:02x}{:02x}{:02x}{:02x}",
			(self.r * 255.0).round() as u8,
			(self.g * 255.0).round() as u8,
			(self.b * 255.0).round() as u8,
			(self.a * 255.0).round() as u8
		)
	}
}

impl FromStr for ColorRGBA {
	type Err = String;

	#[try_fn]
	fn from_str(s: &str) -> Result<Self, Self::Err> {
		if s.len() != 9 {
			return Err("Invalid color format".into());
		}

		let r = u8::from_str_radix(&s[1..3], 16).map_err(|e| e.to_string())?;
		let g = u8::from_str_radix(&s[3..5], 16).map_err(|e| e.to_string())?;
		let b = u8::from_str_radix(&s[5..7], 16).map_err(|e| e.to_string())?;
		let a = u8::from_str_radix(&s[7..9], 16).map_err(|e| e.to_string())?;

		Self {
			r: r as f32 / 255.0,
			g: g as f32 / 255.0,
			b: b as f32 / 255.0,
			a: a as f32 / 255.0
		}
	}
}

#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::variant))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ, CLONE))]
#[cfg_attr(feature = "rune", rune_functions(Self::r_get, Self::r_set, Self::r_from))]
#[derive(Debug, Clone, PartialEq)]
pub enum Variant {
	#[cfg_attr(feature = "rune", rune(constructor))]
	Ref(#[cfg_attr(feature = "rune", rune(get, set))] Option<Ref>),

	#[cfg_attr(feature = "rune", rune(constructor))]
	Resource(#[cfg_attr(feature = "rune", rune(get, set))] Option<ResourceReference>),

	#[cfg_attr(feature = "rune", rune(constructor))]
	Transform(#[cfg_attr(feature = "rune", rune(get, set))] Transform),

	Uuid(Uuid),

	// Rune doesn't need to know about this
	RepositoryId(hitman_bin1::types::repository::ZRepositoryID),

	#[cfg_attr(feature = "rune", rune(constructor))]
	ColorRGB(#[cfg_attr(feature = "rune", rune(get, set))] ColorRGB),

	#[cfg_attr(feature = "rune", rune(constructor))]
	ColorRGBA(#[cfg_attr(feature = "rune", rune(get, set))] ColorRGBA),

	#[cfg_attr(feature = "rune", rune(constructor))]
	PairStringVariant(
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))] EcoString,
		#[cfg_attr(feature = "rune", rune(get, set, boxed))] Box<Variant>
	),

	#[cfg_attr(feature = "rune", rune(constructor))]
	Variant(#[cfg_attr(feature = "rune", rune(get, set, boxed))] Box<Variant>),

	#[cfg_attr(feature = "rune", rune(constructor))]
	Array(
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))] EcoString,
		#[cfg_attr(feature = "rune", rune(get, set))] Vec<Variant>
	),

	Raw(RawVariant)
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for Variant {
	fn schema_name() -> std::borrow::Cow<'static, str> {
		"Variant".into()
	}

	fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
		schemars::json_schema!({
			"type": "object",
			"properties": {
				"type": { "type": "string" },
				"value": {}
			},
			"required": ["type", "value"]
		})
	}
}

#[cfg(feature = "rune")]
impl Variant {
	#[rune::function(instance, path = Self::get)]
	fn r_get(&self) -> rune::Value {
		from_value(to_value(self).unwrap()).unwrap()
	}

	#[rune::function(instance, path = Self::set)]
	fn r_set(&mut self, value: rune::Value) {
		*self = from_value(to_value(value).unwrap()).unwrap();
	}

	#[rune::function(path = Self::from)]
	fn r_from(value: rune::Value) -> Self {
		from_value(to_value(value).unwrap()).unwrap()
	}
}

#[hotpath::measure_all]
impl Variant {
	pub fn variant_type(&self) -> EcoString {
		match self {
			Variant::Ref(_) => "SEntityTemplateReference".into(),
			Variant::Resource(_) => "ZRuntimeResourceID".into(),
			Variant::Transform(_) => "SMatrix43".into(),
			Variant::Uuid(_) => "ZGuid".into(),
			Variant::RepositoryId(_) => "ZRepositoryID".into(),
			Variant::ColorRGB(_) => "SColorRGB".into(),
			Variant::ColorRGBA(_) => "SColorRGBA".into(),
			Variant::PairStringVariant(_, _) => "TPair<ZString,ZVariant>".into(),
			Variant::Variant(_) => "ZVariant".into(),
			Variant::Array(ty, _) => eco_format!("TArray<{ty}>"),
			Variant::Raw(x) => x.variant_type()
		}
	}

	/// Creates a Variant from a raw ZVariant without doing ANY conversion.
	/// You must only use this for types that are usually represented as raw values (e.g. primitives). Types with QN handling will not be converted and will just be wrapped as raw, producing an invalid value.
	pub fn from_raw(value: &ZVariant) -> Self {
		Self::Raw(RawVariant::H3(value.to_owned()))
	}

	#[try_fn]
	#[context("Failure converting game variant value to QN")]
	pub fn from_game(
		value: &ZVariant,
		factory: &STemplateEntityFactory,
		factory_meta: &ResourceMetadata,
		blueprint: &STemplateEntityBlueprint,
		convert_lossless: bool
	) -> Result<Self> {
		if let Some(items) = value.as_vec() {
			Self::Array(
				value
					.variant_type()
					.strip_prefix("TArray<")
					.unwrap()
					.strip_suffix(">")
					.unwrap()
					.into(),
				items
					.into_iter()
					.map(|item| {
						Self::from_game(
							&from_value(json!({ "$type": item.variant_type(), "$val": item.to_serde()? }))?,
							factory,
							factory_meta,
							blueprint,
							convert_lossless
						)
					})
					.collect::<Result<Vec<_>>>()?
			)
		} else if let Some((first, second)) = value.as_ref::<(EcoString, ZVariant)>() {
			Self::PairStringVariant(
				first.into(),
				Self::from_game(second, factory, factory_meta, blueprint, convert_lossless)?.into()
			)
		} else if let Some(value) = value.as_ref::<SEntityTemplateReference>() {
			Self::Ref(Ref::from_game(value, factory, blueprint, factory_meta)?)
		} else if let Some(value) = value.as_ref::<ZRuntimeResourceID>() {
			match value {
				ZRuntimeResourceID {
					id_high: u32::MAX,
					id_low: u32::MAX
				} => Self::Resource(None),

				id => Self::Resource(Some(
					factory_meta
						.references
						.get(id.as_u64() as usize)
						.context("ZRuntimeResourceID referred to non-existent dependency")?
						.to_owned()
				))
			}
		} else if let Some(value) = value.as_ref::<SMatrix43>() {
			Self::Transform(Transform::from_game(value, convert_lossless))
		} else if let Some(value) = value.as_ref::<ZGuid>() {
			Self::Uuid(Uuid::from_fields(
				value._a,
				value._b,
				value._c,
				&[
					value._d, value._e, value._f, value._g, value._h, value._i, value._j, value._k
				]
			))
		} else if let Some(value) = value.as_ref::<SColorRGB>() {
			Self::ColorRGB(ColorRGB {
				r: value.r,
				g: value.g,
				b: value.b
			})
		} else if let Some(value) = value.as_ref::<SColorRGBA>() {
			Self::ColorRGBA(ColorRGBA {
				r: value.r,
				g: value.g,
				b: value.b,
				a: value.a
			})
		} else if let Some(value) = value.as_ref::<ZRepositoryID>() {
			Self::RepositoryId(value.to_owned())
		} else if let Some(value) = value.as_ref::<ZVariant>() {
			Self::Variant(Self::from_game(value, factory, factory_meta, blueprint, convert_lossless)?.into())
		} else {
			Self::Raw(RawVariant::H3(value.to_owned()))
		}
	}

	#[try_fn]
	#[context("Failure converting QN variant value to game")]
	pub fn to_game(
		&self,
		version: GameVersion,
		factory: &STemplateEntityFactory,
		factory_meta: &ResourceMetadata,
		entity_id_to_index_mapping: &HashMap<EntityID, usize, BuildIdentityHasher<u64>>,
		factory_dependencies_index_mapping: &HashMap<RuntimeID, usize, BuildIdentityHasher<u64>>
	) -> Result<ZVariant> {
		match self {
			Self::Ref(value) => ZVariant::new(Ref::to_game_opt(
				value.as_ref(),
				factory,
				factory_meta,
				entity_id_to_index_mapping
			)?),

			Self::Resource(value) => match value {
				Some(value) => {
					let &idx = factory_dependencies_index_mapping
						.get(&value.resource)
						.context("Factory dependency is missing for resource reference")?;

					ZVariant::new(ZRuntimeResourceID::from_u64(idx as u64))
				}

				None => ZVariant::new(ZRuntimeResourceID {
					id_high: u32::MAX,
					id_low: u32::MAX
				})
			},

			Self::Transform(value) => ZVariant::new(value.to_game()),

			Self::Uuid(value) => {
				let (a, b, c, d) = value.as_fields();
				ZVariant::new(ZGuid {
					_a: a,
					_b: b,
					_c: c,
					_d: d[0],
					_e: d[1],
					_f: d[2],
					_g: d[3],
					_h: d[4],
					_i: d[5],
					_j: d[6],
					_k: d[7]
				})
			}

			Self::RepositoryId(value) => ZVariant::new(value.to_owned()),

			Self::ColorRGB(value) => ZVariant::new(SColorRGB {
				r: value.r,
				g: value.g,
				b: value.b
			}),

			Self::ColorRGBA(value) => ZVariant::new(SColorRGBA {
				r: value.r,
				g: value.g,
				b: value.b,
				a: value.a
			}),

			Self::PairStringVariant(first, second) => ZVariant::new((
				first.to_owned(),
				second.to_game(
					version,
					factory,
					factory_meta,
					entity_id_to_index_mapping,
					factory_dependencies_index_mapping
				)?
			)),

			Self::Variant(value) => ZVariant::new(value.to_game(
				version,
				factory,
				factory_meta,
				entity_id_to_index_mapping,
				factory_dependencies_index_mapping
			)?),

			Self::Array(ty, items) => {
				let val = json!({
					"$type": format!("TArray<{ty}>"),
					"$val": items
						.iter()
						.map(|item| {
							item.to_game(version,
								factory,
								factory_meta,
								entity_id_to_index_mapping,
								factory_dependencies_index_mapping
							)
						})
						.collect::<Result<Vec<_>>>()?
						.into_iter()
						.map(|x| x.to_serde())
						.collect::<Result<Vec<_>, _>>()?
				});

				RawVariant::from_value(val)?.to_h3_wrapped(version)?
			}

			Self::Raw(value) => value.to_h3_wrapped(version)?
		}
	}

	pub fn rough_eq(&self, other: &Self) -> bool {
		match (self, other) {
			(Self::Transform(a), Self::Transform(b)) => {
				(a.position.x * 1000.0).round() == (b.position.x * 1000.0).round()
					&& (a.position.y * 1000.0).round() == (b.position.y * 1000.0).round()
					&& (a.position.z * 1000.0).round() == (b.position.z * 1000.0).round()
					&& (a.rotation.x * 1000.0).round() == (b.rotation.x * 1000.0).round()
					&& (a.rotation.y * 1000.0).round() == (b.rotation.y * 1000.0).round()
					&& (a.rotation.z * 1000.0).round() == (b.rotation.z * 1000.0).round()
					&& match (&a.scale, &b.scale) {
						(Some(a_scale), Some(b_scale)) => {
							(a_scale.x * 1000.0).round() == (b_scale.x * 1000.0).round()
								&& (a_scale.y * 1000.0).round() == (b_scale.y * 1000.0).round()
								&& (a_scale.z * 1000.0).round() == (b_scale.z * 1000.0).round()
						}
						(None, None) => true,
						_ => false
					}
			}

			(Self::Ref(a), Self::Ref(b)) => a == b,
			(Self::Resource(a), Self::Resource(b)) => a == b,
			(Self::Uuid(a), Self::Uuid(b)) => a == b,
			(Self::RepositoryId(a), Self::RepositoryId(b)) => a == b,
			(Self::ColorRGB(a), Self::ColorRGB(b)) => {
				(a.r * 255.0).round() == (b.r * 255.0).round()
					&& (a.g * 255.0).round() == (b.g * 255.0).round()
					&& (a.b * 255.0).round() == (b.b * 255.0).round()
			}
			(Self::ColorRGBA(a), Self::ColorRGBA(b)) => {
				(a.r * 255.0).round() == (b.r * 255.0).round()
					&& (a.g * 255.0).round() == (b.g * 255.0).round()
					&& (a.b * 255.0).round() == (b.b * 255.0).round()
					&& (a.a * 255.0).round() == (b.a * 255.0).round()
			}
			(Self::PairStringVariant(a1, a2), Self::PairStringVariant(b1, b2)) => a1 == b1 && a2.rough_eq(b2),
			(Self::Variant(a), Self::Variant(b)) => a.rough_eq(b),
			(Self::Array(_, a_items), Self::Array(_, b_items)) => {
				if a_items.len() != b_items.len() {
					return false;
				}

				for (a_item, b_item) in a_items.iter().zip(b_items.iter()) {
					if !a_item.rough_eq(b_item) {
						return false;
					}
				}

				true
			}
			(Self::Raw(a), Self::Raw(b)) => a == b,
			_ => false
		}
	}
}

impl Serialize for Variant {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer
	{
		let mut state = serializer.serialize_struct("Variant", 2)?;
		state.serialize_field("type", &self.variant_type())?;

		match self {
			Self::Ref(value) => state.serialize_field("value", value)?,
			Self::Resource(value) => state.serialize_field("value", value)?,
			Self::Transform(value) => state.serialize_field("value", value)?,
			Self::Uuid(value) => state.serialize_field("value", value)?,
			Self::RepositoryId(value) => state.serialize_field("value", value)?,
			Self::ColorRGB(value) => state.serialize_field("value", value)?,
			Self::ColorRGBA(value) => state.serialize_field("value", value)?,
			Self::PairStringVariant(first, second) => state.serialize_field("value", &(first, second))?,
			Self::Variant(value) => state.serialize_field("value", value)?,
			Self::Array(_, items) => state.serialize_field(
				"value",
				&items
					.iter()
					.map(|item| match item {
						Self::Ref(value) => to_value(value),
						Self::Resource(value) => to_value(value),
						Self::Transform(value) => to_value(value),
						Self::Uuid(value) => to_value(value),
						Self::RepositoryId(value) => to_value(value.to_string().to_lowercase()),
						Self::ColorRGB(value) => to_value(value),
						Self::ColorRGBA(value) => to_value(value),
						Self::PairStringVariant(first, second) => to_value((first, second)),
						Self::Variant(value) => to_value(value),
						Self::Array(_, items) => to_value(items),
						Self::Raw(value) => value.to_serde()
					})
					.collect::<Result<Vec<_>, _>>()
					.map_err(S::Error::custom)?
			)?,
			Self::Raw(value) => state.serialize_field("value", &value.to_serde().map_err(S::Error::custom)?)?
		}

		state.end()
	}
}

impl<'de> Deserialize<'de> for Variant {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>
	{
		fn parse_with_type<'de, D>(ty: &str, val: Value) -> Result<Variant, D::Error>
		where
			D: serde::Deserializer<'de>
		{
			let res = match ty {
				"SEntityTemplateReference" => Variant::Ref(serde_json::from_value(val).map_err(D::Error::custom)?),
				"ZRuntimeResourceID" => Variant::Resource(serde_json::from_value(val).map_err(D::Error::custom)?),
				"SMatrix43" => Variant::Transform(serde_json::from_value(val).map_err(D::Error::custom)?),
				"ZGuid" => Variant::Uuid(serde_json::from_value(val).map_err(D::Error::custom)?),
				"ZRepositoryID" => Variant::RepositoryId(serde_json::from_value(val).map_err(D::Error::custom)?),
				"SColorRGB" => Variant::ColorRGB(serde_json::from_value(val).map_err(D::Error::custom)?),
				"SColorRGBA" => Variant::ColorRGBA(serde_json::from_value(val).map_err(D::Error::custom)?),
				"TPair<ZString,ZVariant>" => {
					let (first, second): (EcoString, Value) = serde_json::from_value(val).map_err(D::Error::custom)?;

					Variant::PairStringVariant(
						first,
						Box::new(serde_json::from_value(second).map_err(D::Error::custom)?)
					)
				}
				"ZVariant" => Variant::Variant(Box::new(serde_json::from_value(val).map_err(D::Error::custom)?)),

				_ if ty.starts_with("TArray<") && ty.ends_with('>') => {
					let inner = &ty[7..ty.len() - 1];
					let items: Vec<Value> = serde_json::from_value(val).map_err(D::Error::custom)?;
					let mut variants = Vec::with_capacity(items.len());
					for item in items {
						variants.push(parse_with_type::<D>(inner, item)?);
					}
					Variant::Array(inner.into(), variants)
				}

				_ => Variant::Raw(
					RawVariant::from_value(json!({
						"$type": ty,
						"$val": val
					}))
					.map_err(D::Error::custom)?
				)
			};

			Ok(res)
		}

		let mut v = Value::deserialize(deserializer).map_err(D::Error::custom)?;

		let obj = v
			.as_object_mut()
			.ok_or_else(|| D::Error::custom("Variant must be an object"))?;

		let ty = obj
			.remove("type")
			.and_then(|x| x.as_str().map(|s| s.to_string()))
			.ok_or_else(|| D::Error::custom("Variant must have string 'type' field"))?;

		let value = obj
			.remove("value")
			.ok_or_else(|| D::Error::custom("Variant must have 'value' field"))?;

		parse_with_type::<D>(&ty, value)
	}
}

#[derive(Debug, Clone, PartialEq)]
pub enum RawVariant {
	H1(hitman_bin1::game::h1::ZVariant),
	H2(hitman_bin1::game::h2::ZVariant),
	H3(hitman_bin1::game::h3::ZVariant)
}

impl RawVariant {
	/// Get the variant type (i.e., the $type field).
	pub fn variant_type(&self) -> EcoString {
		match self {
			Self::H1(value) => value.variant_type(),
			Self::H2(value) => value.variant_type(),
			Self::H3(value) => value.variant_type()
		}
	}

	/// Serialize the variant's value to a serde_json::Value, without type information (i.e., the $val field).
	pub fn to_serde(&self) -> Result<Value, serde_json::Error> {
		match self {
			Self::H1(value) => value.to_serde(),
			Self::H2(value) => value.to_serde(),
			Self::H3(value) => value.to_serde()
		}
	}

	pub fn from_value(value: Value) -> Result<Self, serde_json::Error> {
		hitman_bin1::game::h3::ZVariant::deserialize(&value)
			.map(Self::H3)
			.or_else(|_| hitman_bin1::game::h2::ZVariant::deserialize(&value).map(Self::H2))
			.or_else(|_| hitman_bin1::game::h1::ZVariant::deserialize(&value).map(Self::H1))
	}

	#[try_fn]
	pub fn to_h3_wrapped(&self, version: GameVersion) -> Result<ZVariant, serde_json::Error> {
		match self {
			RawVariant::H1(value) => match version {
				GameVersion::H1 => value.to_owned().into_inner().into(),
				GameVersion::H2 => from_value::<hitman_bin1::game::h2::ZVariant>(to_value(value)?)?
					.into_inner()
					.into(),
				GameVersion::H3 => from_value::<hitman_bin1::game::h3::ZVariant>(to_value(value)?)?
					.into_inner()
					.into()
			},
			RawVariant::H2(value) => match version {
				GameVersion::H1 => from_value::<hitman_bin1::game::h1::ZVariant>(to_value(value)?)?
					.into_inner()
					.into(),
				GameVersion::H2 => value.to_owned().into_inner().into(),
				GameVersion::H3 => from_value::<hitman_bin1::game::h3::ZVariant>(to_value(value)?)?
					.into_inner()
					.into()
			},
			RawVariant::H3(value) => match version {
				GameVersion::H1 => from_value::<hitman_bin1::game::h1::ZVariant>(to_value(value)?)?
					.into_inner()
					.into(),
				GameVersion::H2 => from_value::<hitman_bin1::game::h2::ZVariant>(to_value(value)?)?
					.into_inner()
					.into(),
				GameVersion::H3 => value.to_owned()
			}
		}
	}
}

impl Serialize for RawVariant {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer
	{
		match self {
			Self::H1(value) => value.serialize(serializer),
			Self::H2(value) => value.serialize(serializer),
			Self::H3(value) => value.serialize(serializer)
		}
	}
}

#[derive(Type, Serialize)]
#[serde(rename = "Variant")]
struct VariantProxy {
	#[serde(rename = "type")]
	ty: String,

	value: Value
}

impl Type for Variant {
	fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
		VariantProxy::definition(types)
	}
}
