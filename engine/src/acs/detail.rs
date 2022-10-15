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

const HUDMSG_LAYER_SHIFT: i32 = 12;
const HUDMSG_LAYER_MASK: i32 = 0x0000F000;

const HUDMSG_VIS_SHIFT: i32 = 16;
const HUDMSG_VIS_MASK: i32 = 0x00070000;

pub(super) struct LocalVars(Vec<i32>);

pub(super) struct LocalArrayEntry {
	pub(super) size: u32,
	pub(super) offset: i32,
}

#[derive(Default)]
pub(super) struct LocalArray {
	pub(super) entries: Vec<LocalArrayEntry>,
}

pub(super) struct ScriptFunction {
	arg_count: u8,
	has_retval: u8,
	import_num: u8,
	local_count: i32,
	address: u32,
	local_array: LocalArray,
}

/*
https://github.com/rust-lang/rust/issues/100878

#[must_use]
pub(super) fn ascii_id(bytes: [u8; 4]) -> u32 {
	(bytes[0] | (bytes[1] << 8) | (bytes[2] << 16) | (bytes[3] << 24)) as u32
}
*/

const STACK_SIZE: usize = 4096;

struct Stack {
	memory: [i32; STACK_SIZE],
	pointer: usize,
}

impl Default for Stack {
	fn default() -> Self {
		Self {
			memory: [0i32; STACK_SIZE],
			pointer: 0,
		}
	}
}

// Intermediate types that match representatons in object files

/// ZDoom's intermediate script representation.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
pub(super) struct ScriptPointerZD {
	pub(super) number: i16,
	pub(super) kind: u16,
	pub(super) arg_count: u32,
	pub(super) address: u32,
}

/// Hexen's original script representation.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
pub(super) struct ScriptPointerH {
	// This script's kind is `number / 1000`.
	pub(super) number: u32,
	pub(super) address: u32,
	pub(super) arg_count: u32,
}

/// ZDoom's current in-file script representation.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
pub(super) struct ScriptPointerI {
	pub(super) number: i16,
	pub(super) kind: u8,
	pub(super) arg_count: u8,
	pub(super) address: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
pub(super) struct ScriptFunctionFileRepr {
	pub(super) arg_count: u8,
	pub(super) local_count: u8,
	pub(super) has_retval: u8,
	pub(super) import_num: u8,
	pub(super) address: u32,
}