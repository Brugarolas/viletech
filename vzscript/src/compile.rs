//! Code that ties together the frontend, mid-section, and backend.

pub(crate) mod builtins;
pub(crate) mod intern;
pub(crate) mod symbol;

#[cfg(test)]
mod test;

use std::{
	any::TypeId,
	hash::{BuildHasherDefault, Hash, Hasher},
};

use append_only_vec::AppendOnlyVec;
use doomfront::{
	rowan::ast::AstNode,
	zdoom::{decorate, inctree::IncludeTree},
};
use parking_lot::Mutex;
use rustc_hash::{FxHashMap, FxHasher};
use util::rstring::RString;

use crate::{
	ast, back::AbiTypes, issue::Issue, rti, sema::CEval, tsys::TypeDef, zname::ZName, ArcGuard,
	FxDashMap, FxDashSet, FxHamt, Project, Version,
};

use self::{
	intern::{NameInterner, NameIx, NsName, SymbolIx},
	symbol::{Definition, FunctionCode, Location, Symbol},
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
	/// One for each library, parallel to [`Self::sources`].
	pub(crate) namespaces: Vec<Scope>,
	pub(crate) symbols: AppendOnlyVec<Symbol>,
	pub(crate) native_ptrs: FxHashMap<&'static str, NativePtr>,
	pub(crate) native_types: FxHashMap<&'static str, NativeType>,
	// Interning
	pub(crate) strings: FxDashSet<RString>,
	pub(crate) names: NameInterner,
	/// Memoized return values of compile-time-evaluated functions.
	pub(crate) memo: FxDashMap<MemoHash, CEval>,
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

		Self {
			sources,
			issues: Mutex::default(),
			stage: Stage::Declaration,
			failed: false,
			namespaces: vec![],
			symbols: AppendOnlyVec::new(),
			native_ptrs: FxHashMap::default(),
			native_types: FxHashMap::default(),
			strings: FxDashSet::default(),
			names: NameInterner::default(),
			memo: FxDashMap::default(),
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
	/// - Function pointers must be `unsafe extern "C"`.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage {
	Declaration,
	Semantic,
	CodeGen,
}

pub(crate) type Scope = FxHamt<NsName, SymbolIx>;

/// The string slice parameter is a path to the calling file,
/// for error reporting purposes.
pub(crate) type CEvalBuiltin = fn(&Compiler, &str, ast::ArgList) -> Result<CEval, ()>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct MemoHash {
	func: u64,
	args: u64,
}

impl MemoHash {
	#[must_use]
	pub(crate) fn new(func: &ArcGuard<Definition>, args: &ast::ArgList) -> Self {
		Self {
			func: {
				let mut hasher = FxHasher::default();
				func.as_ptr().hash(&mut hasher);
				hasher.finish()
			},
			args: {
				let mut hasher = FxHasher::default();
				args.hash(&mut hasher);
				hasher.finish()
			},
		}
	}
}
