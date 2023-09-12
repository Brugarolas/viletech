//! Code that ties together the frontend, mid-section, and backend.

pub(crate) mod builtins;
pub(crate) mod intern;
pub(crate) mod symbol;

#[cfg(test)]
mod test;

use std::any::TypeId;

use append_only_vec::AppendOnlyVec;
use doomfront::zdoom::{decorate, inctree::IncludeTree};
use parking_lot::Mutex;
use rustc_hash::FxHashMap;
use util::rstring::RString;

use crate::{
	back::AbiTypes, issue::Issue, rti, tsys::TypeDef, zname::ZName, FxDashSet, Project, Version,
};

use self::{
	intern::{NameInterner, NameIx, NsName, SymbolIx},
	symbol::{DefIx, Definition, FunctionCode, Location, Symbol},
};

#[derive(Debug)]
pub struct LibSource {
	pub name: String,
	pub version: Version,
	pub native: bool,
	pub inctree: crate::IncludeTree,
	pub decorate: Option<IncludeTree<decorate::Syn>>,
}

#[derive(Debug)]
pub enum NativePtr {
	Data {
		ptr: *const u8,
		layout: AbiTypes,
	},
	Function {
		ptr: *const u8,
		params: AbiTypes,
		returns: AbiTypes,
	},
}

// SAFETY: Caller of `Compiler::native` provides guarantees about given pointers.
unsafe impl Send for NativePtr {}
unsafe impl Sync for NativePtr {}

#[derive(Debug)]
pub struct NativeType {
	id: TypeId,
	layout: AbiTypes,
}

#[derive(Debug)]
pub struct Compiler {
	// Input
	pub(crate) sources: Vec<LibSource>,
	// State
	pub(crate) stage: Stage,
	pub(crate) issues: Mutex<Vec<Issue>>,
	pub(crate) failed: bool,
	// Storage
	pub(crate) builtins: Builtins,
	pub(crate) globals: Scope,
	pub(crate) defs: AppendOnlyVec<Definition>,
	pub(crate) native_ptrs: FxHashMap<&'static str, NativePtr>,
	pub(crate) native_types: FxHashMap<&'static str, NativeType>,
	/// One for each library, parallel to [`Self::sources`].
	pub(crate) namespaces: Vec<Scope>,
	pub(crate) symbols: AppendOnlyVec<Symbol>,
	// Interning
	pub(crate) strings: FxDashSet<RString>,
	pub(crate) names: NameInterner,
}

impl Compiler {
	#[must_use]
	pub fn new(sources: impl IntoIterator<Item = LibSource>) -> Self {
		let sources: Vec<_> = sources
			.into_iter()
			.map(|s| {
				assert!(
					!s.inctree.any_errors(),
					"cannot compile due to parse errors or include tree errors"
				);

				s
			})
			.collect();

		assert!(
			!sources.is_empty(),
			"`Compiler::new` needs at least one `LibSource`"
		);

		#[must_use]
		fn core_t(
			defs: &AppendOnlyVec<Definition>,
			qname: &'static str,
			tdef: &TypeDef,
		) -> rti::Handle<TypeDef> {
			let zname = ZName::from(RString::new(qname));
			let store = rti::Store::new(zname.clone(), tdef.clone());
			let record = rti::Record::new_type(store);
			let handle = record.handle_type();
			let ix = defs.push(Definition::Type { record });
			handle
		}

		let defs = AppendOnlyVec::new();

		let builtins = Builtins {
			void_t: core_t(&defs, "vzs.void", &TypeDef::BUILTIN_VOID),
			bool_t: core_t(&defs, "vzs.bool", &TypeDef::BUILTIN_BOOL),
			int32_t: core_t(&defs, "vzs.int32", &TypeDef::BUILTIN_INT32),
			uint32_t: core_t(&defs, "vzs.uint32", &TypeDef::BUILTIN_UINT32),
			int64_t: core_t(&defs, "vzs.int64", &TypeDef::BUILTIN_INT64),
			uint64_t: core_t(&defs, "vzs.uint64", &TypeDef::BUILTIN_UINT64),
			float32_t: core_t(&defs, "vzs.float32", &TypeDef::BUILTIN_FLOAT32),
			float64_t: core_t(&defs, "vzs.float64", &TypeDef::BUILTIN_FLOAT64),
			iname_t: core_t(&defs, "vzs.iname", &TypeDef::BUILTIN_INAME),
			string_t: core_t(&defs, "vzs.string", &TypeDef::BUILTIN_STRING),
		};

		Self {
			sources,
			issues: Mutex::default(),
			stage: Stage::Declaration,
			failed: false,
			builtins,
			globals: Scope::default(),
			defs,
			native_ptrs: FxHashMap::default(),
			native_types: FxHashMap::default(),
			namespaces: vec![],
			symbols: AppendOnlyVec::new(),
			strings: FxDashSet::default(),
			names: NameInterner::default(),
		}
	}

	/// This is provided as a separate method from [`Self::new`] to:
	/// - isolate unsafe behavior
	/// - allow building the given map in parallel to the declaration pass
	///
	/// # Safety
	///
	/// - Dereferencing a data object pointer or calling a function pointer must
	/// never invoke any thread-unsafe behavior.
	/// - For every value in `ptrs`, one of the provided [`LibSource`]s must
	/// contain a declaration (with no definition) with a `native` attribute,
	/// with a single string argument matching the key in `ptrs`. That
	/// declaration must be ABI-compatible with the native function's raw pointer.
	pub unsafe fn register_native(
		&mut self,
		ptrs: FxHashMap<&'static str, NativePtr>,
		types: FxHashMap<&'static str, NativeType>,
	) {
		assert!(matches!(self.stage, Stage::Declaration | Stage::Semantic));
		self.native_ptrs = ptrs;
		self.native_types = types;
	}

	#[must_use]
	pub fn failed(&self) -> bool {
		self.failed
	}

	pub fn drain_issues(&mut self) -> impl Iterator<Item = Issue> + '_ {
		self.issues.get_mut().drain(..)
	}

	#[must_use]
	pub(crate) fn intern_string(&self, string: &str) -> RString {
		if let Some(ret) = self.strings.get(string) {
			return ret.clone();
		}

		let ret = RString::new(string);
		let _ = self.strings.insert(ret.clone());
		ret
	}

	#[must_use]
	pub(crate) fn get_corelib_type(&self, name: &str) -> &Symbol {
		let nsname = NsName::Type(self.names.intern_str(name));
		let &sym_ix = self.namespaces[0].get(&nsname).unwrap();
		self.symbol(sym_ix)
	}

	#[must_use]
	pub(crate) fn resolve_path(&self, location: Location) -> &str {
		let libsrc = &self.sources[location.lib_ix as usize];
		libsrc.inctree.files[location.file_ix as usize].path()
	}

	#[must_use]
	pub(crate) fn symbol(&self, ix: SymbolIx) -> &Symbol {
		&self.symbols[ix.0 as usize]
	}

	#[must_use]
	pub(crate) fn define_type(&self, qname: &str, tdef: TypeDef) -> (DefIx, rti::Handle<TypeDef>) {
		let zname = ZName::from(RString::new(qname));
		let store = rti::Store::new(zname.clone(), TypeDef::BUILTIN_INT32.clone());
		let record = rti::Record::new_type(store);
		let handle = record.handle_type();
		let ix = self.defs.push(Definition::Type { record });
		debug_assert!(ix < (u32::MAX as usize));
		(DefIx::Some(ix as u32), handle)
	}

	#[must_use]
	pub(crate) fn define_function(
		&self,
		qname: &str,
		tdef: TypeDef,
		code: FunctionCode,
	) -> (DefIx, rti::Handle<TypeDef>) {
		let (_, ty_handle) = self.define_type(qname, tdef);
		let ty_handle = ty_handle.downcast().unwrap();

		let zname = ZName::from(RString::new(qname));
		let store = rti::Store::new(zname.clone(), TypeDef::BUILTIN_INT32.clone());
		let record = rti::Record::new_type(store);
		let handle = record.handle_type();
		let ix = self.defs.push(Definition::Function {
			typedef: ty_handle,
			code,
		});

		debug_assert!(ix < (u32::MAX as usize));
		(DefIx::Some(ix as u32), handle)
	}

	pub(crate) fn raise(&self, issue: Issue) {
		let mut guard = self.issues.lock();
		guard.push(issue);
	}

	#[must_use]
	pub(crate) fn any_errors(&self) -> bool {
		let guard = self.issues.lock();
		guard.iter().any(|iss| iss.is_error())
	}
}

pub(crate) type Scope = FxHashMap<NsName, SymbolIx>;

/// Cache handles to types which will be commonly referenced
/// to keep hash table lookups down.
#[derive(Debug)]
pub(crate) struct Builtins {
	pub(crate) void_t: rti::Handle<TypeDef>,
	pub(crate) bool_t: rti::Handle<TypeDef>,
	pub(crate) int32_t: rti::Handle<TypeDef>,
	pub(crate) uint32_t: rti::Handle<TypeDef>,
	pub(crate) int64_t: rti::Handle<TypeDef>,
	pub(crate) uint64_t: rti::Handle<TypeDef>,
	pub(crate) float32_t: rti::Handle<TypeDef>,
	pub(crate) float64_t: rti::Handle<TypeDef>,
	pub(crate) iname_t: rti::Handle<TypeDef>,
	pub(crate) string_t: rti::Handle<TypeDef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage {
	Declaration,
	Semantic,
	CodeGen,
}
