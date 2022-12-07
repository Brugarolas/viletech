//! Abstraction over the OS file system for security and ease.

/*

Copyright (C) 2022 ***REMOVED***

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program.  If not, see <http://www.gnu.org/licenses/>.

*/

use std::{
	collections::HashMap,
	path::{Path, PathBuf},
};

use fasthash::metro;
use globset::Glob;
use once_cell::sync::Lazy;
use regex::Regex;

mod entry;
mod error;
mod fileref;
mod impure;
mod mount;
#[cfg(test)]
mod test;

use entry::{Entry, EntryKind};

pub use self::impure::{ImpureFileRef, ImpureVfs};
pub use error::Error;
pub use fileref::FileRef;

/// Abstraction over the OS file system for security and ease.
///
/// Inspired by PhysicsFS, but differs in that it owns every byte mounted.
/// Just the mounting process requires large amounts of time spent on file I/O,
/// so clustering a complete read along with it grants a time savings.
pub struct VirtualFs {
	/// The first entry is always the root node. This is of the kind
	/// [`EntryKind::Directory`], and lies under the virtual path `/`.
	entries: Vec<Entry>,
	/// Mounted game data object IDs are used as keys.
	real_paths: HashMap<String, PathBuf>,
}

// Public interface.
impl VirtualFs {
	/// For each tuple of the given slice, `::0` should be the path to the real
	/// file/directory, and `::1` should be the desired "mount point".
	/// Returns a `Vec` parallel to `mounts` which contains an `Ok(())` for each
	/// successful mount and an error otherwise.
	#[must_use]
	pub fn mount(
		&mut self,
		mounts: &[(impl AsRef<Path>, impl AsRef<Path>)],
	) -> Vec<Result<(), Error>> {
		self.mount_parallel(mounts)
	}

	pub fn mount_supported(path: impl AsRef<Path>) -> Result<(), Error> {
		let path = path.as_ref();

		if !path.exists() {
			return Err(Error::NonExistentFile(path.to_owned()));
		}

		if path.is_symlink() {
			return Err(Error::SymlinkMount);
		}

		Ok(())
	}

	/// Returns `None` if and only if nothing exists at the given path.
	#[must_use]
	pub fn lookup(&self, path: impl AsRef<Path>) -> Option<FileRef> {
		let (index, entry) = match self.lookup_hash(Self::hash_path(path)) {
			Some(e) => e,
			None => {
				return None;
			}
		};

		Some(FileRef {
			vfs: self,
			entry,
			handle: Handle(index),
		})
	}

	/// Returns `None` if and only if nothing exists at the given path.
	/// Note that that `path` must be exact, including the root path separator.
	#[must_use]
	pub fn lookup_nocase(&self, path: impl AsRef<Path>) -> Option<FileRef> {
		self.entries
			.iter()
			.enumerate()
			.find(|(_, e)| {
				e.path_str().eq_ignore_ascii_case(
					path.as_ref()
						.to_str()
						.expect("`lookup_nocase` received a path with invalid UTF-8."),
				)
			})
			.map(|(i, e)| FileRef {
				vfs: self,
				entry: e,
				handle: Handle(i),
			})
	}

	pub fn exists(&self, path: impl AsRef<Path>) -> bool {
		self.lookup(path).is_some()
	}

	/// Returns `false` if nothing is at the given path.
	#[must_use]
	pub fn is_dir(&self, path: impl AsRef<Path>) -> bool {
		match self.lookup(path) {
			Some(entry) => entry.is_dir(),
			None => false,
		}
	}

	/// Returns [`Error::NonExistentEntry`] if there's nothing at the supplied path,
	/// or [`Error::Unreadable`] if attempting to read a directory.
	pub fn read(&self, path: impl AsRef<Path>) -> Result<&[u8], Error> {
		let path = path.as_ref();

		let (_, entry) = match self.lookup_hash(Self::hash_path(path)) {
			Some(e) => e,
			None => {
				return Err(Error::NonExistentEntry(path.to_owned()));
			}
		};

		match &entry.kind {
			EntryKind::Binary { .. } | EntryKind::String { .. } => Ok(entry.read()),
			EntryKind::Directory { .. } => Err(Error::Unreadable),
		}
	}

	/// Returns [`Error::InvalidUtf8`] if the contents at the path are not valid UTF-8.
	/// Otherwise acts like [`VirtualFs::read`].
	pub fn read_str(&self, path: impl AsRef<Path>) -> Result<&str, Error> {
		let bytes = self.read(path)?;

		match std::str::from_utf8(bytes) {
			Ok(ret) => Ok(ret),
			Err(_) => Err(Error::InvalidUtf8),
		}
	}

	/// Returns `Some(0)` if the given path is a leaf node.
	/// Returns `None` if and only if nothing exists at the given path.
	#[must_use]
	pub fn count(&self, path: impl AsRef<Path>) -> Option<usize> {
		let (_, entry) = self.lookup_hash(Self::hash_path(path))?;

		if entry.is_leaf() {
			Some(0)
		} else {
			Some(self.children_of(entry).count())
		}
	}

	/// The total number of mounted entries, excluding the root.
	#[must_use]
	pub fn total_count(&self) -> usize {
		self.entries.len() - 1
	}

	/// The number of real files/directories mounted anywhere in the tree.
	#[must_use]
	pub fn mount_count(&self) -> usize {
		self.real_paths.len()
	}

	/// Linear-searches for all entries which match a glob pattern.
	#[must_use]
	pub fn glob(&self, pattern: Glob) -> Option<impl Iterator<Item = FileRef>> {
		let glob = pattern.compile_matcher();

		Some(
			self.entries
				.iter()
				.enumerate()
				.filter(move |(_, e)| glob.is_match(e.path_str()))
				.map(move |(i, e)| FileRef {
					vfs: self,
					entry: e,
					handle: Handle(i),
				}),
		)
	}

	/// Provides quantitative information about the VFS' current internal state.
	#[must_use]
	pub fn diag(&self) -> DiagInfo {
		DiagInfo {
			mount_count: self.real_paths.len(),
			num_entries: self.entries.len(),
			mem_usage: self.mem_usage(&self.entries[0]),
		}
	}
}

impl Default for VirtualFs {
	#[must_use]
	fn default() -> Self {
		VirtualFs {
			entries: vec![Entry::new_dir(PathBuf::from("/"), 0)],
			real_paths: Default::default(),
		}
	}
}

pub struct DiagInfo {
	pub mount_count: usize,
	pub num_entries: usize,
	pub mem_usage: usize,
}

/// An index into the VFS. Allows `O(1)` access to an entry with no borrows.
///
/// Retrieve one via [`FileRef::get_handle`].
///
/// There's nothing preventing these from being invalidated if the VFS is rearranged
/// while one is outstanding, but Impure applications operate on the principle that,
/// excluding the mandatory engine data, files are only mounted when starting a
/// game and unmounted when quitting it, and handles that don't point into `/impure`
/// should only be created during this period and dropped afterwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Handle(pub(super) usize);

// Miscellaneous internal implementation details.
impl VirtualFs {
	/// To make path-hashing flexible over paths that don't include a root path
	/// separator (the VFS never deals in relative paths), the path is hashed
	/// by its components (with a preceding path separator hashed beforehand if
	/// necessary) one at a time, rather than as a whole string.
	#[must_use]
	fn hash_path(path: impl AsRef<Path>) -> u64 {
		let path = path.as_ref();
		let mut hash = 0u64;

		if !path.starts_with("/") {
			hash ^= metro::hash64("/");
		}

		let comps = path.components();

		for comp in comps {
			hash ^= metro::hash64(
				comp.as_os_str()
					.to_str()
					.expect("`hash_path` received a path with invalid UTF-8."),
			);
		}

		hash
	}

	fn children_of<'v>(&'v self, dir: &'v Entry) -> impl Iterator<Item = &'v Entry> {
		self.entries.iter().filter(|e| e.parent_hash == dir.hash)
	}

	#[must_use]
	fn lookup_hash(&self, hash: u64) -> Option<(usize, &Entry)> {
		self.entries
			.iter()
			.enumerate()
			.find(|(_, e)| e.hash == hash)
	}

	/// Recursively gets the total memory usage of a directory.
	#[must_use]
	fn mem_usage(&self, dir: &Entry) -> usize {
		let mut ret = 0;

		for child in self.children_of(dir) {
			ret += std::mem::size_of_val(child);

			match &child.kind {
				EntryKind::Binary(bytes) => {
					ret += bytes.len();
				}
				EntryKind::String(string) => {
					ret += string.len();
				}
				EntryKind::Directory => {
					ret += self.mem_usage(child);
				}
			}
		}

		ret
	}
}

static RGX_INVALIDMOUNTPATH: Lazy<Regex> = Lazy::new(|| {
	Regex::new(r"[^A-Za-z0-9-_/\.]").expect(stringify!(
			"Failed to evaluate regex set: "
			module_path!(),
			":",
			line!(),
			":"
			column!()
	))
});
