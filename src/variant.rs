use std::{
	fmt::{self, Display, Formatter},
	str::FromStr
};

use anyhow::{Context, Result};
use ecow::{EcoString, eco_format};
use glacier_bin1::types::resource::ZRuntimeResourceID;
use glacier_commons::metadata::{ResourceMetadata, ResourceReference, RuntimeID};
use glam::{Affine3, EulerRot, Mat3, Quat};
use serde::{
	Deserialize, Serialize,
	de::Error as _,
	ser::{Error as _, SerializeStruct}
};
use serde_json::{Value, json, to_value};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use specta::Type;
use tryvial::try_fn;
use uuid::Uuid;

use crate::{
	HashMap,
	entity::{EntityID, Ref},
	game::{FromQuickEntity, ToQuickEntity, types as game_types}
};

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

	pub fn to_glam(&self) -> Affine3 {
		let scale = if let Some(scale) = self.scale {
			scale.into()
		} else {
			glam::Vec3 { x: 1.0, y: 1.0, z: 1.0 }
		};

		Affine3::from_scale_rotation_translation(scale, self.rotation.into(), self.position.into())
	}
}

mod transform_impl {
	use super::*;

	macro_rules! impl_game {
		($game:ident) => {
			impl ToQuickEntity for glacier_bin1::game::$game::SMatrix43 {
				type QuickEntity = Transform;
				type Error = !;

				type Factory = game_types::$game::Factory;
				type Blueprint = game_types::$game::Blueprint;

				#[try_fn]
				fn to_qn(
					&self,
					_: &Self::Factory,
					_: &ResourceMetadata,
					_: &Self::Blueprint,
					_: &ResourceMetadata,
					lossless: bool
				) -> Result<Self::QuickEntity, Self::Error> {
					// Mat3 is column-major while SMatrix43 is row-major, so we have to transpose
					let matrix = Mat3 {
						x_axis: glam::Vec3 {
							x: self.x_axis.x,
							y: self.y_axis.x,
							z: self.z_axis.x
						},
						y_axis: glam::Vec3 {
							x: self.x_axis.y,
							y: self.y_axis.y,
							z: self.z_axis.y
						},
						z_axis: glam::Vec3 {
							x: self.x_axis.z,
							y: self.y_axis.z,
							z: self.z_axis.z
						}
					};

					Transform::from_glam(
						Affine3::from_mat3_translation(
							if matrix.determinant() == 0.0
								|| matrix.x_axis.length() == 0.0
								|| matrix.y_axis.length() == 0.0
								|| matrix.z_axis.length() == 0.0
							{
								// Reset invalid rotations to identity
								Mat3::IDENTITY
							} else {
								matrix
							},
							glam::Vec3 {
								x: self.trans.x,
								y: self.trans.y,
								z: self.trans.z
							}
						),
						lossless
					)
				}
			}

			impl FromQuickEntity<Transform> for glacier_bin1::game::$game::SMatrix43 {
				type Error = !;

				#[try_fn]
				fn from_qn(
					trans: &Transform,
					_: &HashMap<EntityID, usize>,
					_: &HashMap<RuntimeID, usize>,
					_: &HashMap<RuntimeID, usize>
				) -> Result<Self, Self::Error> {
					let transform = trans.to_glam();

					// Transpose
					Self {
						x_axis: glacier_bin1::game::$game::SVector3 {
							x: transform.matrix3.x_axis.x,
							y: transform.matrix3.y_axis.x,
							z: transform.matrix3.z_axis.x
						},
						y_axis: glacier_bin1::game::$game::SVector3 {
							x: transform.matrix3.x_axis.y,
							y: transform.matrix3.y_axis.y,
							z: transform.matrix3.z_axis.y
						},
						z_axis: glacier_bin1::game::$game::SVector3 {
							x: transform.matrix3.x_axis.z,
							y: transform.matrix3.y_axis.z,
							z: transform.matrix3.z_axis.z
						},
						trans: glacier_bin1::game::$game::SVector3 {
							x: transform.translation.x,
							y: transform.translation.y,
							z: transform.translation.z
						}
					}
				}
			}
		};
	}

	#[cfg(feature = "h1")]
	impl_game!(h1);

	#[cfg(feature = "h2")]
	impl_game!(h2);

	#[cfg(feature = "h3")]
	impl_game!(h3);

	#[cfg(feature = "fl")]
	impl_game!(fl);
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

mod color_impl {
	use super::*;

	macro_rules! impl_game {
		($game:ident) => {
			impl ToQuickEntity for glacier_bin1::game::$game::SColorRGB {
				type QuickEntity = ColorRGB;
				type Error = !;

				type Factory = game_types::$game::Factory;
				type Blueprint = game_types::$game::Blueprint;

				#[try_fn]
				fn to_qn(
					&self,
					_: &Self::Factory,
					_: &ResourceMetadata,
					_: &Self::Blueprint,
					_: &ResourceMetadata,
					_: bool
				) -> Result<Self::QuickEntity, Self::Error> {
					ColorRGB {
						r: self.r,
						g: self.g,
						b: self.b
					}
				}
			}

			impl FromQuickEntity<ColorRGB> for glacier_bin1::game::$game::SColorRGB {
				type Error = !;

				#[try_fn]
				fn from_qn(
					color: &ColorRGB,
					_: &HashMap<EntityID, usize>,
					_: &HashMap<RuntimeID, usize>,
					_: &HashMap<RuntimeID, usize>
				) -> Result<Self, Self::Error> {
					Self {
						r: color.r,
						g: color.g,
						b: color.b
					}
				}
			}

			impl ToQuickEntity for glacier_bin1::game::$game::SColorRGBA {
				type QuickEntity = ColorRGBA;
				type Error = !;

				type Factory = game_types::$game::Factory;
				type Blueprint = game_types::$game::Blueprint;

				#[try_fn]
				fn to_qn(
					&self,
					_: &Self::Factory,
					_: &ResourceMetadata,
					_: &Self::Blueprint,
					_: &ResourceMetadata,
					_: bool
				) -> Result<Self::QuickEntity, Self::Error> {
					ColorRGBA {
						r: self.r,
						g: self.g,
						b: self.b,
						a: self.a
					}
				}
			}

			impl FromQuickEntity<ColorRGBA> for glacier_bin1::game::$game::SColorRGBA {
				type Error = !;

				#[try_fn]
				fn from_qn(
					color: &ColorRGBA,
					_: &HashMap<EntityID, usize>,
					_: &HashMap<RuntimeID, usize>,
					_: &HashMap<RuntimeID, usize>
				) -> Result<Self, Self::Error> {
					Self {
						r: color.r,
						g: color.g,
						b: color.b,
						a: color.a
					}
				}
			}
		};
	}

	#[cfg(feature = "h1")]
	impl_game!(h1);

	#[cfg(feature = "h2")]
	impl_game!(h2);

	#[cfg(feature = "h3")]
	impl_game!(h3);

	#[cfg(feature = "fl")]
	impl_game!(fl);
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
		serde_json::from_value(to_value(self).unwrap()).unwrap()
	}

	#[rune::function(instance, path = Self::set)]
	fn r_set(&mut self, value: rune::Value) {
		*self = serde_json::from_value(to_value(value).unwrap()).unwrap();
	}

	#[rune::function(path = Self::from)]
	fn r_from(value: rune::Value) -> Self {
		serde_json::from_value(to_value(value).unwrap()).unwrap()
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
			Variant::ColorRGB(_) => "SColorRGB".into(),
			Variant::ColorRGBA(_) => "SColorRGBA".into(),
			Variant::PairStringVariant(_, _) => "TPair<ZString,ZVariant>".into(),
			Variant::Variant(_) => "ZVariant".into(),
			Variant::Array(ty, _) => eco_format!("TArray<{ty}>"),
			Variant::Raw(x) => x.variant_type().into()
		}
	}

	/// Directly wrap a raw ZVariant without applying QN conversion.
	pub fn from_raw(raw: impl Into<RawVariant>) -> Self {
		Self::Raw(raw.into())
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

mod variant_impl {
	use super::*;

	macro_rules! impl_game {
		($game:ident, $game_uppercase:ident) => {
			impl ToQuickEntity for glacier_bin1::game::$game::ZVariant {
				type QuickEntity = Variant;
				type Error = anyhow::Error;

				type Factory = game_types::$game::Factory;
				type Blueprint = game_types::$game::Blueprint;

				#[try_fn]
				fn to_qn(
					&self,
					factory: &Self::Factory,
					factory_meta: &ResourceMetadata,
					blueprint: &Self::Blueprint,
					blueprint_meta: &ResourceMetadata,
					lossless: bool
				) -> Result<Self::QuickEntity, Self::Error> {
					if let Some(items) = self.as_vec() {
						Variant::Array(
							self.variant_type()
								.strip_prefix("TArray<")
								.unwrap()
								.strip_suffix(">")
								.unwrap()
								.into(),
							items
								.into_iter()
								.map(|item| {
									Self::from(item.clone_underlying()).to_qn(
										factory,
										factory_meta,
										blueprint,
										blueprint_meta,
										lossless
									)
								})
								.collect::<Result<Vec<_>>>()?
						)
					} else {
						if let Some((first, second)) = self.as_ref::<(EcoString, Self)>() {
							return Ok(Variant::PairStringVariant(
								first.into(),
								second
									.to_qn(factory, factory_meta, blueprint, blueprint_meta, lossless)?
									.into()
							));
						}

						if let Some(value) = self.as_ref::<glacier_bin1::game::$game::SEntityTemplateReference>() {
							Variant::Ref(value.to_qn(factory, factory_meta, blueprint, blueprint_meta, lossless)?)
						} else if let Some(value) = self.as_ref::<ZRuntimeResourceID>() {
							match value {
								ZRuntimeResourceID {
									id_high: u32::MAX,
									id_low: u32::MAX
								} => Variant::Resource(None),

								id => Variant::Resource(Some(
									factory_meta
										.references
										.get(id.as_u64() as usize)
										.with_context(|| format!("No such reference with index {}", id.as_u64()))?
										.to_owned()
								))
							}
						} else if let Some(value) = self.as_ref::<glacier_bin1::game::$game::SMatrix43>() {
							Variant::Transform(value.to_qn(
								factory,
								factory_meta,
								blueprint,
								blueprint_meta,
								lossless
							)?)
						} else if let Some(value) = self.as_ref::<glacier_bin1::game::$game::ZGuid>() {
							Variant::Uuid(Uuid::from_fields(
								value._a,
								value._b,
								value._c,
								&[
									value._d, value._e, value._f, value._g, value._h, value._i, value._j, value._k
								]
							))
						} else if let Some(value) = self.as_ref::<glacier_bin1::game::$game::SColorRGB>() {
							Variant::ColorRGB(value.to_qn(
								factory,
								factory_meta,
								blueprint,
								blueprint_meta,
								lossless
							)?)
						} else if let Some(value) = self.as_ref::<glacier_bin1::game::$game::SColorRGBA>() {
							Variant::ColorRGBA(value.to_qn(
								factory,
								factory_meta,
								blueprint,
								blueprint_meta,
								lossless
							)?)
						} else if let Some(value) = self.as_ref::<Self>() {
							Variant::Variant(
								value
									.to_qn(factory, factory_meta, blueprint, blueprint_meta, lossless)?
									.into()
							)
						} else {
							Variant::Raw(RawVariant::$game_uppercase(self.clone()))
						}
					}
				}
			}

			impl FromQuickEntity<Variant> for glacier_bin1::game::$game::ZVariant {
				type Error = anyhow::Error;

				#[try_fn]
				fn from_qn(
					variant: &Variant,
					entity_indices: &HashMap<EntityID, usize>,
					reference_indices: &HashMap<RuntimeID, usize>,
					external_scene_indices: &HashMap<RuntimeID, usize>
				) -> Result<Self, Self::Error> {
					match variant {
						Variant::Ref(value) => Self::new(glacier_bin1::game::$game::SEntityTemplateReference::from_qn(
							value,
							entity_indices,
							reference_indices,
							external_scene_indices
						)?),

						Variant::Resource(value) => Self::new(match value {
							Some(value) => {
								let &idx = reference_indices.get(&value.resource).unwrap();

								ZRuntimeResourceID::from_u64(idx as u64)
							}

							None => ZRuntimeResourceID {
								id_high: u32::MAX,
								id_low: u32::MAX
							}

						}),

						Variant::Transform(value) => Self::new(glacier_bin1::game::$game::SMatrix43::from_qn(
							value,
							entity_indices,
							reference_indices,
							external_scene_indices
						)?),

						Variant::Uuid(value) => {
							let (a, b, c, d) = value.as_fields();
							Self::new(glacier_bin1::game::$game::ZGuid {
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

						Variant::ColorRGB(value) => Self::new(glacier_bin1::game::$game::SColorRGB::from_qn(
							value,
							entity_indices,
							reference_indices,
							external_scene_indices
						)?),

						Variant::ColorRGBA(value) => Self::new(glacier_bin1::game::$game::SColorRGBA::from_qn(
							value,
							entity_indices,
							reference_indices,
							external_scene_indices
						)?),

						Variant::PairStringVariant(first, second) => Self::new((
							first.to_owned(),
							Self::from_qn(
								second,
								entity_indices,
								reference_indices,
								external_scene_indices
							)?
						)),

						Variant::Variant(value) => {
							Self::new(Self::from_qn(
								value,
								entity_indices,
								reference_indices,
								external_scene_indices
							)?)
						}

						Variant::Array(ty, items) => {
							let val = json!({
								"$type": format!("TArray<{ty}>"),
								"$val": items
									.iter()
									.map(|item| {
										Self::from_qn(
											item,
											entity_indices,reference_indices,
											external_scene_indices
										)
									})
									.collect::<Result<Vec<_>>>()?
									.into_iter()
									.map(|x| x.to_serde())
									.collect::<Result<Vec<_>, _>>()?
							});

							serde_json::from_value(val)?
						}

						Variant::Raw(value) => match value {
							RawVariant::$game_uppercase(raw) => raw.clone(),
							_ => serde_json::from_value(serde_json::to_value(value)?)?
						}
					}
				}
			}
		};
	}

	#[cfg(feature = "h1")]
	impl_game!(h1, H1);

	#[cfg(feature = "h2")]
	impl_game!(h2, H2);

	#[cfg(feature = "h3")]
	impl_game!(h3, H3);

	#[cfg(feature = "fl")]
	impl_game!(fl, FL);
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

				_ => Variant::Raw(RawVariant::Unknown(ty.into(), val))
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

#[cfg(feature = "h1")]
impl From<glacier_bin1::game::h1::ZVariant> for RawVariant {
	fn from(value: glacier_bin1::game::h1::ZVariant) -> Self {
		Self::H1(value)
	}
}

#[cfg(feature = "h2")]
impl From<glacier_bin1::game::h2::ZVariant> for RawVariant {
	fn from(value: glacier_bin1::game::h2::ZVariant) -> Self {
		Self::H2(value)
	}
}

#[cfg(feature = "h3")]
impl From<glacier_bin1::game::h3::ZVariant> for RawVariant {
	fn from(value: glacier_bin1::game::h3::ZVariant) -> Self {
		Self::H3(value)
	}
}

#[cfg(feature = "fl")]
impl From<glacier_bin1::game::fl::ZVariant> for RawVariant {
	fn from(value: glacier_bin1::game::fl::ZVariant) -> Self {
		Self::FL(value)
	}
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum RawVariant {
	#[cfg(feature = "h1")]
	H1(glacier_bin1::game::h1::ZVariant),

	#[cfg(feature = "h2")]
	H2(glacier_bin1::game::h2::ZVariant),

	#[cfg(feature = "h3")]
	H3(glacier_bin1::game::h3::ZVariant),

	#[cfg(feature = "fl")]
	FL(glacier_bin1::game::fl::ZVariant),

	Unknown(EcoString, Value)
}

impl PartialEq for RawVariant {
	fn eq(&self, other: &Self) -> bool {
		match (self, other) {
			#[cfg(feature = "h1")]
			(Self::H1(a), Self::H1(b)) => a == b,

			#[cfg(feature = "h2")]
			(Self::H2(a), Self::H2(b)) => a == b,

			#[cfg(feature = "h3")]
			(Self::H3(a), Self::H3(b)) => a == b,

			#[cfg(feature = "fl")]
			(Self::FL(a), Self::FL(b)) => a == b,

			_ => self.variant_type() == other.variant_type() && self.to_serde().ok() == other.to_serde().ok()
		}
	}
}

impl RawVariant {
	/// Get the variant type (i.e., the $type field).
	pub fn variant_type(&self) -> &str {
		match self {
			#[cfg(feature = "h1")]
			Self::H1(value) => value.variant_type(),

			#[cfg(feature = "h2")]
			Self::H2(value) => value.variant_type(),

			#[cfg(feature = "h3")]
			Self::H3(value) => value.variant_type(),

			#[cfg(feature = "fl")]
			Self::FL(value) => value.variant_type(),

			Self::Unknown(ty, _) => ty
		}
	}

	/// Serialize the variant's value to a serde_json::Value, without type information (i.e., the $val field).
	pub fn to_serde(&self) -> Result<Value, serde_json::Error> {
		match self {
			#[cfg(feature = "h1")]
			Self::H1(value) => value.to_serde(),

			#[cfg(feature = "h2")]
			Self::H2(value) => value.to_serde(),

			#[cfg(feature = "h3")]
			Self::H3(value) => value.to_serde(),

			#[cfg(feature = "fl")]
			Self::FL(value) => value.to_serde(),

			Self::Unknown(_, val) => Ok(val.to_owned())
		}
	}
}

impl Serialize for RawVariant {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer
	{
		match self {
			#[cfg(feature = "h1")]
			Self::H1(value) => value.serialize(serializer),

			#[cfg(feature = "h2")]
			Self::H2(value) => value.serialize(serializer),

			#[cfg(feature = "h3")]
			Self::H3(value) => value.serialize(serializer),

			#[cfg(feature = "fl")]
			Self::FL(value) => value.serialize(serializer),

			Self::Unknown(ty, val) => json!({
				"$type": ty,
				"$val": val
			})
			.serialize(serializer)
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
