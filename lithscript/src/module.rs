//! [`Module`] and its [`Builder`].

use std::{mem::MaybeUninit, sync::Arc};

use cranelift_jit::JITBuilder;

/// A single compilation unit, corresponding to one source file.
#[derive(Debug)]
pub struct Module {
	/// Fully-qualified, e.g. `lithscript/core`.
	_name: String,
	/// A hash derived from this module's entire [source](Source).
	_checksum: u64,
	#[allow(unused)]
	jit: Arc<JitModule>,
}

/// Used to prepare for compiling a [`Module`], primarily by registering native
/// functions to be callable by scripts.
pub struct Builder {
	name: String,
	_jit: JITBuilder,
}

impl Builder {
	/// Also see [`JITBuilder::hotswap`].
	#[must_use]
	pub fn new(name: String, hotswap: bool) -> Self {
		let mut jit = JITBuilder::new(cranelift_module::default_libcall_names())
			.expect("failed to construct a Cranelift `JITBuilder`");

		jit.hotswap(hotswap);

		Self { name, _jit: jit }
	}
}

impl std::fmt::Debug for Builder {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Builder")
			.field("name", &self.name)
			.field("jit", &"<cranelift_jit::JITBuilder>")
			.finish()
	}
}

// Details /////////////////////////////////////////////////////////////////////

/// Ensures proper JIT code de-allocation when all [`sym::Store`]s
/// (and, by extension, all [`Handle`]s) drop their `Arc`s.
#[derive(Debug)]
struct JitModule(MaybeUninit<cranelift_jit::JITModule>);

impl Drop for JitModule {
	fn drop(&mut self) {
		unsafe {
			let i = std::mem::replace(&mut self.0, MaybeUninit::uninit());
			i.assume_init().free_memory();
		}
	}
}

// SAFETY: The content of this structure is only ever mutated before ending up
// behind an `Arc`, after which point it is immutable until destruction.
unsafe impl Send for JitModule {}
unsafe impl Sync for JitModule {}
