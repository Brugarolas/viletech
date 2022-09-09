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

use std::path::Path;

use regex::Regex;

use crate::utils::path::str_iter_from_path;

use super::{entry::{Entry, EntryKind}, error::Error, VirtualFs};

#[derive(Clone)]
pub struct Handle<'v, 'e> {
	pub(super) vfs: &'v VirtualFs,
	pub(super) entry: &'e Entry,
}

impl<'v, 'e> Handle<'v, 'e> {
	pub fn lookup(&self, path: impl AsRef<Path>) -> Option<Handle> {
		self.lookup_recur(str_iter_from_path(path.as_ref()))
	}

	pub fn lookup_nocase(&self, path: impl AsRef<Path>) -> Option<Handle> {
		self.lookup_recur_nocase(str_iter_from_path(path.as_ref()))
	}

	pub fn read(&self) -> Result<&[u8], Error> {
		match &self.entry.kind {
			EntryKind::Directory { .. } => Err(Error::Unreadable),
			EntryKind::Leaf { bytes } => Ok(&bytes[..]),
		}
	}

	/// Returns [`Error::InvalidUtf8`] if the entry's contents aren't valid UTF-8.
	/// Otherwise acts like [`Handle::read`].
	pub fn read_str(&self) -> Result<&str, Error> {
		match std::str::from_utf8(self.read()?) {
			Ok(ret) => Ok(ret),
			Err(_) => Err(Error::InvalidUtf8),
		}
	}

	/// Returns [`Error::Unreadable`] if attempting to read a directory.
	pub fn copy(&self) -> Result<Vec<u8>, Error> {
		match &self.entry.kind {
			EntryKind::Directory { .. } => Err(Error::Unreadable),
			EntryKind::Leaf { bytes } => Ok(bytes.clone()),
		}
	}

	/// Returns [`Error::InvalidUtf8`] if the entry's contents aren't valid UTF-8.
	/// Otherwise acts like [`Handle::copy`].
	pub fn copy_string(&self) -> Result<String, Error> {
		match String::from_utf8(self.copy()?) {
			Ok(ret) => Ok(ret),
			Err(_) => Err(Error::InvalidUtf8),
		}
	}

	pub fn children(&'e self) -> impl Iterator<Item = Handle> {
		self.child_entries().map(|e| Handle {
				vfs: self.vfs,
				entry: e,
			})
	}

	/// Note: non-recursive. Panics if used on a leaf node.
	/// Check to ensure it's a directory beforehand.
	pub fn contains(&self, name: &str) -> bool {
		self.child_entries().any(|e| e.file_name() == name)
	}

	/// Note: non-recursive. Panics if used on a leaf node.
	/// Check to ensure it's a directory beforehand.
	pub fn contains_regex(&self, regex: &Regex) -> bool {
		self.children().any(|h| regex.is_match(h.file_name()))
	}

	pub fn count(&self) -> usize {
		match &self.entry.kind {
			EntryKind::Leaf { .. } => 0,
			EntryKind::Directory { .. } => self.child_entries().count()
		}
	}

	pub fn virtual_path(&self) -> &'e Path {
		&self.entry.path
	}

	pub fn path_str(&self) -> &'e str {
		self.entry.path_str()
	}

	pub fn file_name(&self) -> &str {
		self.entry.file_name()
	}

	pub fn is_dir(&self) -> bool {
		self.entry.is_dir()
	}

	pub fn is_leaf(&self) -> bool {
		self.entry.is_leaf()
	}
}

// Internal implementation details.
impl<'v, 'e> Handle<'v, 'e> {
	fn child_entries(&'e self) -> impl Iterator<Item = &'e Entry> {
		self.vfs
			.entries
			.iter()
			.filter(|e| e.parent_hash == self.entry.hash)
	}

	fn lookup_recur<'s>(&self, mut iter: impl Iterator<Item = &'s str>) -> Option<Handle> {
		let comp = match iter.next() {
			Some(c) => c,
			None => { return Some(self.clone()); }
		};

		for entry in self.child_entries() {
			if entry.file_name() != comp {
				continue;
			}

			return self.lookup_recur(iter);
		}

		None
	}

	fn lookup_recur_nocase<'s>(&self, mut iter: impl Iterator<Item = &'s str>) -> Option<Handle> {
		let comp = match iter.next() {
			Some(c) => c,
			None => { return Some(self.clone()); }
		};

		for entry in self.child_entries() {
			if !entry.file_name().eq_ignore_ascii_case(comp)  {
				continue;
			}

			return self.lookup_recur_nocase(iter);
		}

		None
	}
}