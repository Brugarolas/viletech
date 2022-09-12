//! Trait extending [`mlua::Lua`] with Impure-specific behavior.

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

use crate::vfs::VirtualFs;
use log::{debug, error, info, warn};
use mlua::prelude::*;
use parking_lot::RwLock;
use std::{
	sync::Arc,
	time::{SystemTime, UNIX_EPOCH},
};

/// Only exists to extends [`mlua::Lua`] with new methods.
pub trait ImpureLua<'p> {
	/// Seeds the RNG, defines some dependency-free global functions (logging, etc.).
	/// If `safe` is `false`, the debug and FFI libraries are loaded.
	/// If `clientside` is `true`, the state's registry will contain the key-value
	/// pair `['clientside'] = true`. Otherwise, this key will be left nil.
	fn new_ex(safe: bool, clientside: bool) -> LuaResult<Lua>;

	/// Modifies the Lua global environment to be more conducive to a safe,
	/// Impure-suitable sandbox, and adds numerous Impure-specific symbols.
	fn global_init(&self, vfs: Arc<RwLock<VirtualFs>>) -> LuaResult<()>;

	/// Adds `math`, `string`, and `table` standard libraries to an environment,
	/// as well as several standard free functions and `_VERSION`.
	fn envbuild_std(&self, env: &LuaTable);

	/// For guaranteeing that loaded chunks are text.
	fn safeload<'lua, 'a, S>(
		&'lua self,
		chunk: &'a S,
		name: &str,
		env: LuaTable<'lua>,
	) -> LuaChunk<'lua, 'a>
	where
		S: mlua::AsChunk<'lua> + ?Sized;

	fn teal_compile(&self, source: &str) -> LuaResult<String>;
}

impl<'p> ImpureLua<'p> for mlua::Lua {
	fn new_ex(safe: bool, clientside: bool) -> LuaResult<Lua> {
		// Note: `io`, `os`, and `package` aren't sandbox-safe by themselves.
		// They either get pruned of dangerous functions by `global_init` or
		// are deleted now and may get returned in reduced form in the future.

		#[rustfmt::skip]
		let safe_libs =
			LuaStdLib::BIT |
			LuaStdLib::IO |
			LuaStdLib::JIT |
			LuaStdLib::MATH |
			LuaStdLib::OS |
			LuaStdLib::PACKAGE |
			LuaStdLib::STRING |
			LuaStdLib::TABLE;

		let ret = if let true = safe {
			Lua::new_with(safe_libs, LuaOptions::default())?
		} else {
			unsafe {
				Lua::unsafe_new_with(
					safe_libs | LuaStdLib::DEBUG | LuaStdLib::FFI,
					LuaOptions::default(),
				)
			}
		};

		if clientside {
			ret.set_named_registry_value("clientside", true)
				.expect("`ImpureLua::new_ex` failed to set state ID in registry.");
		}

		// Seed the Lua's random state for trivial (i.e. client-side) purposes

		{
			let rseed: LuaFunction = ret
				.globals()
				.get::<_, LuaTable>("math")?
				.get::<_, LuaFunction>("randomseed")?;
			let seed = SystemTime::now()
				.duration_since(UNIX_EPOCH)
				.expect("Failed to retrieve system time.")
				.as_millis() as u32;
			match rseed.call::<u32, ()>(seed) {
				Ok(()) => {}
				Err(err) => warn!("Failed to seed a Lua state's RNG: {}", err),
			};
		}

		let impure = match ret.create_table() {
			Ok(t) => t,
			Err(err) => {
				error!("Failed to create global table `impure`.");
				return Err(err);
			}
		};

		impure.set(
			"log",
			ret.create_function(|_, msg: String| {
				info!("{}", msg);
				Ok(())
			})?,
		)?;

		impure.set(
			"warn",
			ret.create_function(|_, msg: String| {
				warn!("{}", msg);
				Ok(())
			})?,
		)?;

		impure.set(
			"err",
			ret.create_function(|_, msg: String| {
				error!("{}", msg);
				Ok(())
			})?,
		)?;

		impure.set(
			"debug",
			ret.create_function(|_, msg: String| {
				debug!("{}", msg);
				Ok(())
			})?,
		)?;

		impure.set(
			"version",
			ret.create_function(|_, _: ()| {
				Ok((
					env!("CARGO_PKG_VERSION_MAJOR"),
					env!("CARGO_PKG_VERSION_MINOR"),
					env!("CARGO_PKG_VERSION_PATCH"),
				))
			})?,
		)?;

		ret.globals().set("impure", impure)?;

		Ok(ret)
	}

	fn global_init(&self, vfs: Arc<RwLock<VirtualFs>>) -> LuaResult<()> {
		fn delete_g(globals: &LuaTable) -> LuaResult<()> {
			// Many functions (e.g. `jit`, `setfenv`) aren't deleted here,
			// but aren't included in any user-facing environment

			const KEYS_STD_GLOBAL: [&str; 5] = [
				"io",
				"package",
				// Free functions
				"collectgarbage",
				"module",
				"print",
			];

			for key in KEYS_STD_GLOBAL {
				globals.set(key, LuaValue::Nil)?;
			}

			Ok(())
		}

		fn delete_g_os(globals: &LuaTable) -> LuaResult<()> {
			const KEYS_STD_OS: [&str; 7] = [
				"execute",
				"exit",
				"getenv",
				"remove",
				"rename",
				"setlocale",
				"tmpname",
			];

			let g_os: LuaTable = globals.get("os")?;

			for key in KEYS_STD_OS {
				g_os.set(key, LuaValue::Nil)?;
			}

			Ok(())
		}

		fn g_import(lua: &Lua, globals: &LuaTable, vfs: Arc<RwLock<VirtualFs>>) -> LuaResult<()> {
			globals.set(
				"import",
				lua.create_function(move |l, path: String| -> LuaResult<LuaValue> {
					let vfs = vfs.read();

					let bytes = match vfs.read(&path) {
						Ok(b) => b,
						Err(err) => {
							return Err(LuaError::ExternalError(Arc::new(err)));
						}
					};

					let string = match std::str::from_utf8(bytes) {
						Ok(s) => s,
						Err(err) => {
							return Err(LuaError::ExternalError(Arc::new(err)));
						}
					};

					let chunk = match l.teal_compile(string) {
						Ok(s) => s,
						Err(err) => {
							return Err(LuaError::ExternalError(Arc::new(err)));
						}
					};

					let env = l
						.globals()
						.call_function("getenv", 0)
						.expect("`import` failed to retrieve the current environment.");

					return l.safeload(&chunk, path.as_str(), env).eval();
				})?,
			)
		}

		fn g_vfs_read(lua: &Lua, g_vfs: &LuaTable, vfs: Arc<RwLock<VirtualFs>>) -> LuaResult<()> {
			let func = lua.create_function(move |l, path: String| {
				let vfs = vfs.read();

				let handle = match vfs.lookup(&path) {
					Some(h) => h,
					None => {
						return Ok(LuaValue::Nil);
					}
				};

				let content = match handle.read_str() {
					Ok(s) => s,
					Err(err) => {
						error!(
							"File contents are invalid UTF-8: {}
							Error: {}",
							path, err
						);
						return Ok(LuaValue::Nil);
					}
				};

				let string = match l.create_string(content) {
					Ok(s) => s,
					Err(err) => {
						return Err(err);
					}
				};

				Ok(LuaValue::String(string))
			})?;

			g_vfs.set("read", func)
		}

		fn teal(lua: &Lua, compat53: LuaTable) -> LuaResult<()> {
			let env = lua.create_table()?;

			for pair in lua.globals().pairs::<LuaValue, LuaValue>() {
				let (key, value) = pair?;
				env.set(key, value)?;
			}

			env.set("compat53", compat53)?;

			let teal: LuaTable = lua
				.safeload(include_str!("./teal.lua"), "teal", env)
				.eval()?;

			lua.globals().set("teal", teal)
		}

		let globals = self.globals();
		let g_vfs = self.create_table()?;
		// compat53 module gets privileged access to symbols which are later deleted.
		// This only gets referenced by the Teal compiler, so nothing unsafe leaks
		let compat53 = self
			.safeload(include_str!("./compat53.lua"), "compat53", self.globals())
			.eval::<LuaTable>()?;

		delete_g(&globals)?;
		delete_g_os(&globals)?;
		g_import(self, &globals, vfs.clone())?;
		g_vfs_read(self, &g_vfs, vfs.clone())?;
		teal(self, compat53)?;

		globals.set("vfs", g_vfs)?;

		// Teal "prelude": container utilities (map/array) /////////////////////

		let vfsg = vfs.read();

		let array = vfsg
			.read_str("/impure/lua/array.tl")
			.or_else(|err| Err(LuaError::ExternalError(Arc::new(err))))?;
		let array = self.teal_compile(array)?;
		let array = self.safeload(&array, "array", globals.clone());
		let array: LuaTable = array.eval()?;
		globals.set("array", array)?;

		let map = vfsg
			.read_str("/impure/lua/map.tl")
			.or_else(|err| Err(LuaError::ExternalError(Arc::new(err))))?;
		let map = self.teal_compile(map)?;
		let map = self.safeload(&map, "map", globals.clone());
		let map: LuaTable = map.eval()?;
		globals.set("map", map)?;

		drop(vfsg);

		Ok(())
	}

	fn envbuild_std(&self, env: &LuaTable) {
		debug_assert!(
			env.raw_len() <= 0,
			"`ImpureLua::env_init_std`: Called on a non-empty table."
		);

		let globals = self.globals();

		const GLOBAL_KEYS: [&str; 16] = [
			"_VERSION",
			// Tables
			"math",
			"string",
			"table",
			// Free functions
			"error",
			"getmetatable",
			"ipairs",
			"next",
			"pairs",
			"pcall",
			"select",
			"tonumber",
			"tostring",
			"type",
			"unpack",
			"xpcall",
		];

		for key in GLOBAL_KEYS {
			let func = globals
				.get::<&str, LuaValue>(key)
				.expect("`ImpureLua::env_init_std`: global `{}` is missing.");

			env.set(key, func).unwrap_or_else(|err| {
				panic!(
					"`ImpureLua::env_init_std`: failed to set `{}` ({}).",
					key, err
				)
			});
		}

		let debug: LuaResult<LuaTable> = globals.get("debug");

		if let Ok(d) = debug {
			env.set("debug", d)
				.expect("`ImpureLua::env_init_std`: Failed to set `debug`.");
		}
	}

	fn safeload<'lua, 'a, S>(
		&'lua self,
		chunk: &'a S,
		name: &str,
		env: LuaTable<'lua>,
	) -> LuaChunk<'lua, 'a>
	where
		S: mlua::AsChunk<'lua> + ?Sized,
	{
		self.load(chunk)
			.set_mode(mlua::ChunkMode::Text)
			.set_environment(env)
			.expect("`ImpureLua::safeload()`: Got malformed environment.")
			.set_name(name)
			.expect("`ImpureLua::safeload()`: Got unsanitised name.")
	}

	fn teal_compile(&self, source: &str) -> LuaResult<String> {
		self.globals()
			.get::<&str, LuaTable>("teal")
			.expect("Teal compiler hasn't been exported yet.")
			.get::<&str, LuaFunction>("gen")
			.expect("Teal compiler is missing function: `gen`.")
			.call::<&str, String>(source)
	}
}