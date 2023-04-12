//! Management of files, audio, graphics, levels, text, localization, and so on.

pub mod asset;
mod detail;
mod error;
mod ext;
mod interface;
mod mount;
mod prep;
#[cfg(test)]
mod test;
mod vfs;

use std::{path::Path, sync::Arc};

use bevy_egui::egui;
use dashmap::DashMap;
use globset::Glob;
use indexmap::IndexMap;
use parking_lot::{Mutex, RwLock};
use rayon::prelude::*;
use regex::Regex;
use smallvec::SmallVec;

use crate::{utils::path::PathExt, vzs, EditorNum, SpawnNum, VPath, VPathBuf};

pub use self::{asset::*, error::*, ext::*, interface::*, vfs::*};

use self::detail::{AssetKey, AssetSlotKey, Config, VfsKey};

/// The data catalog is the heart of file and asset management in VileTech.
/// "Physical" files are "mounted" into one cohesive virtual file system (VFS)
/// tree that makes it easy for all other parts of the engine to access any given
/// unit of data, without exposing any details of the user's real underlying machine.
///
/// A mounted file or directory has the same tree structure in the virtual FS as
/// in the physical one, although binary files are converted into more useful
/// forms (e.g. decoding sounds and images) if their format can be easily identified.
/// Otherwise, they're left as-is.
///
/// Any given unit of data or [`Asset`] is stored behind an [`Arc`], allowing
/// other parts of the engine to take out high-speed [`Handle`]s to something and
/// safely access it without passing through locks or casts.
///
/// A footnote on semantics: it is impossible to mount a file that's nested within
/// an archive. If `mymod.zip` contains `myothermod.vpk7`, there's no way to
/// register `myothermod` as a mount in the official sense. It's just a part of
/// `mymod`'s file tree.
#[derive(Debug)]
pub struct Catalog {
	pub(self) config: Config,
	pub(self) vzscript: vzs::Project,
	/// Element 0 is always the root node, under virtual path `/`.
	///
	/// The choice to use an `IndexMap` here is very deliberate.
	/// - Directory contents can be stored in an alphabetically-sorted way.
	/// - Ordering is preserved for WAD entries.
	/// - Exact-path lookups are fast.
	/// - Memory contiguity means that linear searches are non-pessimized.
	/// - If a load fails, restoring the previous state is simple truncation.
	pub(self) files: IndexMap<VfsKey, File>,
	/// The first element is always the engine's base data (ID `viletech`),
	/// but every following element is user-specified, including their order.
	pub(self) mounts: Vec<Mount>,
	/// In each value:
	/// - Field `0` is an index into `Self::mounts`.
	/// - Field `1` is an index into [`Mount::assets`].
	pub(self) assets: DashMap<AssetKey, (usize, AssetSlotKey)>,
	/// Asset lookup table without namespacing. Thus, requesting `MAP01` returns
	/// the last element in the array behind that key, as doom.exe would if
	/// loading multiple WADs with similarly-named entries.
	pub(self) nicknames: DashMap<AssetKey, SmallVec<[(usize, AssetSlotKey); 2]>>,
	/// See the key type's documentation for background details.
	pub(self) editor_nums: DashMap<EditorNum, SmallVec<[(usize, AssetSlotKey); 2]>>,
	/// See the key type's documentation for background details.
	pub(self) spawn_nums: DashMap<SpawnNum, SmallVec<[(usize, AssetSlotKey); 2]>>,
	// Q: FNV/aHash for maps using small key types?
}

impl Catalog {
	/// This is an end-to-end function that reads physical files, fills out the
	/// VFS, and then processes the files to decompose them into assets.
	/// Much of the important things to know are in the documentation for
	/// [`LoadRequest`]. The range of possible errors is documented by
	/// [`MountError`] and [`PrepError`].
	///
	/// Notes:
	/// - The order of pre-existing VFS entries and mounts is unchanged upon success.
	/// - This function is partially atomic. If mounting fails, the catalog's
	/// state is left entirely unchanged from before calling this.
	/// If asset preparation fails, the VFS state is not restored to before the
	/// call as a form of memoization, allowing future prep attempts to skip most
	/// mounting work (to allow faster mod development cycles).
	/// - Each load request is fulfilled in parallel using [`rayon`]'s global
	/// thread pool, but the caller thread itself gets blocked.
	#[must_use = "loading may return errors which should be handled"]
	pub fn load<RP, MP>(&mut self, request: LoadRequest<RP, MP>) -> LoadOutcome
	where
		RP: AsRef<Path>,
		MP: AsRef<VPath>,
	{
		let new_mounts = self.mounts.len()..(self.mounts.len() + request.paths.len());
		let mnt_ctx = mount::Context::new(request.tracker);

		// Note to reader: check `./mount.rs`.
		let mnt_output = match self.mount(&request.paths, mnt_ctx) {
			detail::Outcome::Ok(output) => output,
			detail::Outcome::Err(errors) => return LoadOutcome::MountFail { errors },
			detail::Outcome::Cancelled => return LoadOutcome::Cancelled,
			detail::Outcome::None => unreachable!(),
		};

		// Note to reader: check `./prep.rs`.
		let p_ctx = prep::Context::new(mnt_output.tracker, new_mounts);

		match self.prep(p_ctx) {
			detail::Outcome::Ok(output) => LoadOutcome::Ok {
				mount: mnt_output.errors,
				prep: output.errors,
			},
			detail::Outcome::Err(errors) => LoadOutcome::PrepFail { errors },
			detail::Outcome::Cancelled => LoadOutcome::Cancelled,
			detail::Outcome::None => unreachable!(),
		}
	}

	/// Keep the first `len` mounts. Remove the rest, along their files.
	/// If `len` is greater than the number of mounts, this function is a no-op.
	pub fn truncate(&mut self, len: usize) {
		if len == 0 {
			self.files.clear();
			self.mounts.clear();
			return;
		} else if len >= self.mounts.len() {
			return;
		}

		for mount in self.mounts.drain(len..) {
			let vpath = mount.info.virtual_path();

			self.files.retain(|_, entry| !entry.path.is_child_of(vpath));
		}

		self.clear_dirs();
		self.populate_dirs();
		self.clean_maps();
	}

	#[must_use]
	pub fn get_file(&self, path: impl AsRef<VPath>) -> Option<FileRef> {
		self.files.get(&VfsKey::new(path)).map(|file| FileRef {
			catalog: self,
			file,
		})
	}

	/// Note that `A` here is a filter on the type that comes out of the lookup,
	/// rather than an assertion that the asset under `id` is that type, so this
	/// returns an `Option` rather than a [`Result`].
	#[must_use]
	pub fn get_asset<A: Asset>(&self, id: &str) -> Option<&Arc<A>> {
		let key = AssetKey::new::<A>(id);

		if let Some(kvp) = self.assets.get(&key) {
			self.mounts[kvp.0].assets[kvp.1].as_any().downcast_ref()
		} else {
			None
		}
	}

	/// Find an [`Actor`] [`Blueprint`] by a 16-bit editor number.
	/// The last blueprint assigned the given number is what gets returned.
	///
	/// [`Actor`]: crate::sim::actor::Actor
	#[must_use]
	pub fn bp_by_ednum(&self, num: EditorNum) -> Option<&Arc<Blueprint>> {
		self.editor_nums.get(&num).map(|kvp| {
			let stack = kvp.value();
			let last = stack
				.last()
				.expect("Catalog cleanup missed an empty ed-num stack.");
			self.mounts[last.0].assets[last.1]
				.as_any()
				.downcast_ref()
				.unwrap()
		})
	}

	/// Find an [`Actor`] [`Blueprint`] by a 16-bit spawn number.
	/// The last blueprint assigned the given number is what gets returned.
	///
	/// [`Actor`]: crate::sim::actor::Actor
	#[must_use]
	pub fn bp_by_spawnnum(&self, num: SpawnNum) -> Option<&Arc<Blueprint>> {
		self.spawn_nums.get(&num).map(|kvp| {
			let stack = kvp.value();
			let last = stack
				.last()
				.expect("Catalog cleanup missed an empty spawn-num stack.");
			self.mounts[last.0].assets[last.1]
				.as_any()
				.downcast_ref()
				.unwrap()
		})
	}

	#[must_use]
	pub fn file_exists(&self, path: impl AsRef<VPath>) -> bool {
		self.files.contains_key(&VfsKey::new(path))
	}

	pub fn all_files(&self) -> impl Iterator<Item = FileRef> {
		self.files.iter().map(|(_, file)| FileRef {
			catalog: self,
			file,
		})
	}

	/// Note that WAD files will be yielded out of their original order, and
	/// all other files will not exhibit the alphabetical sorting with which
	/// they are internally stored.
	#[must_use = "iterators are lazy and do nothing unless consumed"]
	pub fn all_files_par(&self) -> impl ParallelIterator<Item = FileRef> {
		self.all_files().par_bridge()
	}

	pub fn get_files_glob(&self, pattern: Glob) -> impl Iterator<Item = FileRef> {
		let glob = pattern.compile_matcher();

		self.files.iter().filter_map(move |(_, file)| {
			if glob.is_match(&file.path) {
				Some(FileRef {
					catalog: self,
					file,
				})
			} else {
				None
			}
		})
	}

	/// Note that WAD files will be yielded out of their original order, and
	/// all other files will not exhibit the alphabetical sorting with which
	/// they are internally stored.
	#[must_use = "iterators are lazy and do nothing unless consumed"]
	pub fn get_files_glob_par(&self, pattern: Glob) -> impl ParallelIterator<Item = FileRef> {
		self.get_files_glob(pattern).par_bridge()
	}

	pub fn get_files_regex(&self, pattern: Regex) -> impl Iterator<Item = FileRef> {
		self.files.iter().filter_map(move |(_, file)| {
			if pattern.is_match(file.path_str()) {
				Some(FileRef {
					catalog: self,
					file,
				})
			} else {
				None
			}
		})
	}

	/// Note that WAD files will be yielded out of their original order, and
	/// all other files will not exhibit the alphabetical sorting with which
	/// they are internally stored.
	#[must_use = "iterators are lazy and do nothing unless consumed"]
	pub fn get_files_regex_par(&self, pattern: Regex) -> impl ParallelIterator<Item = FileRef> {
		self.get_files_regex(pattern).par_bridge()
	}

	#[must_use]
	pub fn last_asset_by_nick<A: Asset>(&self, nick: &str) -> Option<&Arc<A>> {
		let key = AssetKey::new::<A>(nick);

		self.nicknames.get(&key).map(|kvp| {
			let stack = kvp.value();
			let last = stack
				.last()
				.expect("Catalog cleanup missed an empty nickname stack.");
			self.mounts[last.0].assets[last.1]
				.as_any()
				.downcast_ref()
				.unwrap()
		})
	}

	#[must_use]
	pub fn first_asset_by_nick<A: Asset>(&self, nick: &str) -> Option<&Arc<A>> {
		let key = AssetKey::new::<A>(nick);

		self.nicknames.get(&key).map(|kvp| {
			let stack = kvp.value();
			let last = stack
				.first()
				.expect("Catalog cleanup missed an empty nickname stack.");
			self.mounts[last.0].assets[last.1]
				.as_any()
				.downcast_ref()
				.unwrap()
		})
	}

	#[must_use]
	pub fn mounts(&self) -> &[Mount] {
		&self.mounts
	}

	#[must_use]
	pub fn config_get(&self) -> ConfigGet {
		ConfigGet(self)
	}

	#[must_use]
	pub fn config_set(&mut self) -> ConfigSet {
		ConfigSet(self)
	}

	/// The returned value reflects only the footprint of the content of the
	/// virtual files themselves; the size of the data structures isn't included,
	/// since it's trivial next to the size of large text files and binary blobs.
	#[must_use]
	pub fn vfs_mem_usage(&self) -> usize {
		self.files
			.par_values()
			.fold(|| 0_usize, |acc, file| acc + file.byte_len())
			.sum()
	}

	/// Draw the egui-based developer/debug/diagnosis menu for the VFS.
	pub fn ui_vfs(&self, ctx: &egui::Context, ui: &mut egui::Ui) {
		self.ui_vfs_impl(ctx, ui);
	}

	pub fn ui_assets(&self, ctx: &egui::Context, ui: &mut egui::Ui) {
		self.ui_assets_impl(ctx, ui);
	}
}

impl Default for Catalog {
	fn default() -> Self {
		let root = File {
			path: VPathBuf::from("/").into_boxed_path(),
			kind: FileKind::Directory(vec![]),
		};

		let key = VfsKey::new(&root.path);

		Self {
			config: Config::default(),
			vzscript: vzs::Project::default(),
			files: indexmap::indexmap! { key => root },
			mounts: vec![],
			assets: DashMap::default(),
			nicknames: DashMap::default(),
			editor_nums: DashMap::default(),
			spawn_nums: DashMap::default(),
		}
	}
}

/// A type alias for convenience and to reduce line noise.
pub type CatalogAM = Arc<Mutex<Catalog>>;
/// A type alias for convenience and to reduce line noise.
pub type CatalogAL = Arc<RwLock<Catalog>>;

#[derive(Debug)]
pub enum LoadOutcome {
	/// A cancel was requested externally.
	/// The catalog's state was left unchanged.
	Cancelled,
	/// One or more fatal errors prevented a successful VFS mount.
	MountFail {
		/// Every *new* mount gets a sub-vec, but that sub-vec may be empty.
		errors: Vec<Vec<MountError>>,
	},
	/// Mounting succeeeded, but one or more fatal errors
	/// prevented successful asset preparation.
	PrepFail {
		/// Every *new* mount gets a sub-vec, but that sub-vec may be empty.
		errors: Vec<Vec<PrepError>>,
	},
	/// Loading was successful, but non-fatal errors or warnings may have arisen.
	Ok {
		/// Every *new* mount gets a sub-vec, but that sub-vec may be empty.
		mount: Vec<Vec<MountError>>,
		/// Every *new* mount gets a sub-vec, but that sub-vec may be empty.
		prep: Vec<Vec<PrepError>>,
	},
}

impl LoadOutcome {
	#[must_use]
	pub fn num_errs(&self) -> usize {
		match self {
			LoadOutcome::Cancelled => 0,
			LoadOutcome::MountFail { errors } => {
				errors.iter().fold(0, |acc, subvec| acc + subvec.len())
			}
			LoadOutcome::PrepFail { errors } => {
				errors.iter().fold(0, |acc, subvec| acc + subvec.len())
			}
			LoadOutcome::Ok { mount, prep } => {
				mount.iter().fold(0, |acc, subvec| acc + subvec.len())
					+ prep.iter().fold(0, |acc, subvec| acc + subvec.len())
			}
		}
	}
}

// (RAT) If you're reading this, congratulations! You've found something special.
// This module sub-tree is, historically speaking, the most tortured code in VileTech.
// The Git history doesn't even reflect half of the reworks the VFS has undergone.
