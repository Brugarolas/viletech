//! Level state for the playsim and renderer.
//!
//! While not strictly necessarily, making this a part of the ECS allows use of
//! Bevy's ECS hierarchies to easily clean up an entire level recursively with
//! one call.

use std::{collections::HashMap, sync::Arc};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
	data::{self, asset},
	sparse::{SparseSet, SparseSetIndex},
};

use super::{line, sector::Sector, ActiveMarker};

/// Strongly-typed [`Entity`] wrapper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Level(Entity);

/// The principal component in a level entity.
#[derive(Component, Debug)]
pub struct Core {
	pub base: Option<data::Handle<asset::Level>>,
	pub flags: Flags,
	/// Time spent in this level thus far.
	pub ticks_elapsed: u64,
	pub geom: Geometry,
}

/// Sub-structure for composing [`Core`].
///
/// The vertex array, trigger map, and some counters.
#[derive(Debug)]
pub struct Geometry {
	pub verts: SparseSet<VertIndex, Vertex>,
	pub sides: SparseSet<SideIndex, Side>,
	/// Each stored entity ID points to a sector.
	///
	/// When a line is triggered (walked over, interacted-with, shot), all sectors
	/// in the corresponding array have all "activatable" components get activated.
	pub triggers: HashMap<line::Trigger, Vec<Sector>>,
	/// Updated as map geometry changes.
	pub num_sectors: usize,
}

bitflags::bitflags! {
	#[derive(Default)]
	pub struct Flags: u8 {
		// From GZ. Purpose unclear.
		const FROZEN_LOCAL = 1 << 0;
		// From GZ. Purpose unclear.
		const FROZEN_GLOBAL = 1 << 1;
		/// Monsters which teleport so as to have bounding box intersection with
		/// a player actor kill that actor. Primarily for use in Doom 2's MAP30.
		const MONSTERS_TELEFRAG = 1 << 2;
	}
}

// Vertex information //////////////////////////////////////////////////////////

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Vertex(pub Vec3);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct VertIndex(usize);

impl From<VertIndex> for usize {
	fn from(value: VertIndex) -> Self {
		value.0
	}
}

impl SparseSetIndex for VertIndex {}

// Line sides //////////////////////////////////////////////////////////////////

#[derive(Debug)]
pub struct Side {
	/// Which level does this side belong to?
	pub level: Level,
	pub offset: IVec2,
	pub sector: Sector,
	pub udmf: Udmf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SideIndex(usize);

impl From<SideIndex> for usize {
	fn from(value: SideIndex) -> Self {
		value.0
	}
}

impl SparseSetIndex for SideIndex {}

// UDMF ////////////////////////////////////////////////////////////////////////

/// A map of arbitrary string-keyed values defined in a UDMF TEXTMAP file.
///
/// Can be attached to a line, side, or sector.
#[derive(Component, Debug)]
pub struct Udmf(HashMap<Arc<str>, UdmfValue>);

#[derive(Debug)]
pub enum UdmfValue {
	Int(i32),
	Float(f64),
	String(Arc<str>),
}

pub fn build(mut cmds: Commands, base: data::Handle<asset::Level>, active: bool) {
	let mut cmds_level = cmds.spawn(Core {
		base: Some(base.clone()),
		flags: Flags::empty(),
		ticks_elapsed: 0,
		geom: Geometry {
			verts: SparseSet::default(),
			sides: SparseSet::default(),
			triggers: HashMap::default(),
			num_sectors: base.sectors.len(),
		},
	});

	let matmesh = MaterialMeshBundle::<StandardMaterial>::default();

	// TODO:
	// - Custom material with shaders for per-side/per-sector images.
	// - Assemble mesh from verts, sides, sectors.

	cmds_level.insert(matmesh);

	if active {
		cmds_level.insert(ActiveMarker);
	}

	let _level_id = cmds_level.id();
}
