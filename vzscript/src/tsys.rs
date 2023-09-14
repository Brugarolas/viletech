//! Type information, used for compilation as well as RTTI.

use std::{marker::PhantomData, mem::ManuallyDrop};

use util::rstring::RString;

use crate::{back::AbiTypes, compile::intern::NameIx, rti};

/// No VZScript type is allowed to exceed this size in bytes.
pub const MAX_SIZE: usize = 1024 * 2;

pub struct TypeDef {
	tag: TypeTag,
	data: TypeData,
}

impl rti::RtInfo for TypeDef {}

impl TypeDef {
	#[must_use]
	pub fn abi(&self) -> AbiTypes {
		unsafe {
			match self.tag {
				TypeTag::Array => todo!(),
				TypeTag::Class => todo!(),
				TypeTag::Function => todo!(),
				TypeTag::Primitive => todo!(),
				TypeTag::Struct => todo!(),
				TypeTag::Union => todo!(),
			}
		}
	}

	#[must_use]
	pub fn layout(&self) -> std::alloc::Layout {
		let _ = self.abi();
		todo!()
	}

	pub fn inner(&self) -> TypeRef {
		unsafe {
			match self.tag {
				TypeTag::Array => TypeRef::Array(&self.data.array),
				TypeTag::Class => TypeRef::Class(&self.data.class),
				TypeTag::Function => TypeRef::Function(&self.data.func),
				TypeTag::Primitive => TypeRef::Primitive(&self.data.primitive),
				TypeTag::Struct => TypeRef::Struct(&self.data.structure),
				TypeTag::Union => TypeRef::Union(&self.data.r#union),
			}
		}
	}

	#[must_use]
	pub(crate) fn new_array(array_t: ArrayType) -> Self {
		Self {
			tag: TypeTag::Array,
			data: TypeData {
				array: ManuallyDrop::new(array_t),
			},
		}
	}

	#[must_use]
	pub(crate) fn new_class(class_t: ClassType) -> Self {
		Self {
			tag: TypeTag::Class,
			data: TypeData {
				class: ManuallyDrop::new(class_t),
			},
		}
	}
}

impl Clone for TypeDef {
	fn clone(&self) -> Self {
		Self {
			tag: self.tag,
			data: unsafe {
				match self.tag {
					TypeTag::Array => TypeData {
						array: self.data.array.clone(),
					},
					TypeTag::Class => TypeData {
						class: self.data.class.clone(),
					},
					TypeTag::Function => TypeData {
						func: self.data.func.clone(),
					},
					TypeTag::Primitive => TypeData {
						primitive: self.data.primitive,
					},
					TypeTag::Struct => TypeData {
						structure: self.data.structure.clone(),
					},
					TypeTag::Union => TypeData {
						r#union: self.data.union.clone(),
					},
				}
			},
		}
	}
}

#[derive(Debug)]
pub enum TypeRef<'td> {
	Array(&'td ArrayType),
	Class(&'td ClassType),
	Function(&'td FuncType),
	Primitive(&'td PrimitiveType),
	Struct(&'td StructType),
	Union(&'td UnionType),
}

/// Corresponds to the concept of "scope" in ZScript (renamed to reduce name overloading).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restrict {
	Ui,
	/// i.e. ZScript's "play" scope.
	Sim,
	/// ZScript's "virtual" scope.
	Virtual,
	/// i.e. ZScript's "clearscope".
	None,
}

// TypeData ////////////////////////////////////////////////////////////////////

/// Gets discriminated with [`TypeTag`].
union TypeData {
	array: ManuallyDrop<ArrayType>,
	class: ManuallyDrop<ClassType>,
	func: ManuallyDrop<FuncType>,
	structure: ManuallyDrop<StructType>,
	primitive: ManuallyDrop<PrimitiveType>,
	r#union: ManuallyDrop<UnionType>,
}

/// Separated discriminant for [`TypeData`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TypeTag {
	Array,
	Class,
	Function,
	Primitive,
	Struct,
	Union,
}

impl Drop for TypeDef {
	fn drop(&mut self) {
		unsafe {
			match self.tag {
				TypeTag::Array => ManuallyDrop::drop(&mut self.data.array),
				TypeTag::Class => ManuallyDrop::drop(&mut self.data.class),
				TypeTag::Function => ManuallyDrop::drop(&mut self.data.func),
				TypeTag::Primitive => ManuallyDrop::drop(&mut self.data.primitive),
				TypeTag::Struct => ManuallyDrop::drop(&mut self.data.structure),
				TypeTag::Union => ManuallyDrop::drop(&mut self.data.r#union),
			}
		}
	}
}

impl std::fmt::Debug for TypeDef {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		unsafe {
			f.debug_struct("TypeDef")
				.field("tag", &self.tag)
				.field(
					"data",
					match &self.tag {
						TypeTag::Array => &self.data.array,
						TypeTag::Class => &self.data.class,
						TypeTag::Function => &self.data.func,
						TypeTag::Primitive => &self.data.primitive,
						TypeTag::Struct => &self.data.structure,
						TypeTag::Union => &self.data.r#union,
					},
				)
				.finish()
		}
	}
}

// TypeData's contents /////////////////////////////////////////////////////////

#[derive(Debug, Clone)]
pub struct ArrayType {
	pub len: usize,
	pub elem: rti::InHandle<TypeDef>,
}

#[derive(Debug, Clone)]
pub struct ClassType {
	pub parent: Option<TypeInHandle<ClassType>>,
	pub is_abstract: bool,
	pub restrict: Restrict,
}

#[derive(Debug, Clone)]
pub struct EnumType {}

#[derive(Debug, Clone)]
pub struct FuncType {
	pub params: Vec<Parameter>,
	pub ret: rti::InHandle<TypeDef>,
}

#[derive(Debug, Clone)]
pub struct Parameter {
	pub typedef: rti::Handle<TypeDef>,
	pub optional: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum PrimitiveType {
	Bool,
	Int8,
	Uint8,
	Int16,
	Uint16,
	Int32,
	Uint32,
	Int64,
	Uint64,
	Int128,
	Uint128,
	Float32,
	Float64,

	IName,
	String,
	TypeDef,
	Void,
}

impl PrimitiveType {
	#[must_use]
	pub fn int_bit_width(self) -> Option<u16> {
		match self {
			Self::Int8 | Self::Uint8 => Some(8),
			Self::Int16 | Self::Uint16 => Some(16),
			Self::Int32 | Self::Uint32 => Some(32),
			Self::Int64 | Self::Uint64 => Some(64),
			Self::Int128 | Self::Uint128 => Some(128),
			Self::Float32
			| Self::Float64
			| Self::IName
			| Self::String
			| Self::TypeDef
			| Self::Void
			| Self::Bool => None,
		}
	}
}

#[derive(Debug, Clone)]
pub struct StructType {}

#[derive(Debug, Clone)]
pub struct UnionType {}

// TypeHandle //////////////////////////////////////////////////////////////////

/// Specialization on [`crate::rti::Handle`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeHandle<T>(pub(crate) rti::Handle<TypeDef>, pub(crate) PhantomData<T>);

impl<T> TypeHandle<T> {
	#[must_use]
	pub fn upcast(self) -> rti::Handle<TypeDef> {
		self.0
	}
}

// SAFETY: Whenever dereferencing `TypeHandle`, union accesses are guaranteed
// to be sound because a handle can not be created for the wrong type.

impl std::ops::Deref for TypeHandle<ArrayType> {
	type Target = ArrayType;

	fn deref(&self) -> &Self::Target {
		unsafe { &self.0.data.array }
	}
}

impl std::ops::Deref for TypeHandle<ClassType> {
	type Target = ClassType;

	fn deref(&self) -> &Self::Target {
		unsafe { &self.0.data.class }
	}
}

impl std::ops::Deref for TypeHandle<FuncType> {
	type Target = FuncType;

	fn deref(&self) -> &Self::Target {
		unsafe { &self.0.data.func }
	}
}

impl std::ops::Deref for TypeHandle<PrimitiveType> {
	type Target = PrimitiveType;

	fn deref(&self) -> &Self::Target {
		unsafe { &self.0.data.primitive }
	}
}

impl std::ops::Deref for TypeHandle<StructType> {
	type Target = StructType;

	fn deref(&self) -> &Self::Target {
		unsafe { &self.0.data.structure }
	}
}

impl std::ops::Deref for TypeHandle<UnionType> {
	type Target = UnionType;

	fn deref(&self) -> &Self::Target {
		unsafe { &self.0.data.r#union }
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeInHandle<T>(rti::InHandle<TypeDef>, PhantomData<T>);

/// Primitives.
impl TypeDef {
	pub(crate) const PRIMITIVE_TYPEDEF: Self = Self {
		tag: TypeTag::Primitive,
		data: TypeData {
			primitive: ManuallyDrop::new(PrimitiveType::TypeDef),
		},
	};

	pub(crate) const PRIMITIVE_VOID: Self = Self {
		tag: TypeTag::Primitive,
		data: TypeData {
			primitive: ManuallyDrop::new(PrimitiveType::Void),
		},
	};

	// Numeric /////////////////////////////////////////////////////////////////

	pub(crate) const PRIMITIVE_BOOL: Self = Self {
		tag: TypeTag::Primitive,
		data: TypeData {
			primitive: ManuallyDrop::new(PrimitiveType::Bool),
		},
	};

	pub(crate) const PRIMITIVE_INT8: Self = Self {
		tag: TypeTag::Primitive,
		data: TypeData {
			primitive: ManuallyDrop::new(PrimitiveType::Int8),
		},
	};

	pub(crate) const PRIMITIVE_UINT8: Self = Self {
		tag: TypeTag::Primitive,
		data: TypeData {
			primitive: ManuallyDrop::new(PrimitiveType::Uint8),
		},
	};

	pub(crate) const PRIMITIVE_INT16: Self = Self {
		tag: TypeTag::Primitive,
		data: TypeData {
			primitive: ManuallyDrop::new(PrimitiveType::Int16),
		},
	};

	pub(crate) const PRIMITIVE_UINT16: Self = Self {
		tag: TypeTag::Primitive,
		data: TypeData {
			primitive: ManuallyDrop::new(PrimitiveType::Uint16),
		},
	};

	pub(crate) const PRIMITIVE_INT32: Self = Self {
		tag: TypeTag::Primitive,
		data: TypeData {
			primitive: ManuallyDrop::new(PrimitiveType::Int32),
		},
	};

	pub(crate) const PRIMITIVE_UINT32: Self = Self {
		tag: TypeTag::Primitive,
		data: TypeData {
			primitive: ManuallyDrop::new(PrimitiveType::Uint32),
		},
	};

	pub(crate) const PRIMITIVE_INT64: Self = Self {
		tag: TypeTag::Primitive,
		data: TypeData {
			primitive: ManuallyDrop::new(PrimitiveType::Int64),
		},
	};

	pub(crate) const PRIMITIVE_UINT64: Self = Self {
		tag: TypeTag::Primitive,
		data: TypeData {
			primitive: ManuallyDrop::new(PrimitiveType::Uint64),
		},
	};

	pub(crate) const PRIMITIVE_INT128: Self = Self {
		tag: TypeTag::Primitive,
		data: TypeData {
			primitive: ManuallyDrop::new(PrimitiveType::Int128),
		},
	};

	pub(crate) const PRIMITIVE_UINT128: Self = Self {
		tag: TypeTag::Primitive,
		data: TypeData {
			primitive: ManuallyDrop::new(PrimitiveType::Uint128),
		},
	};

	pub(crate) const PRIMITIVE_FLOAT32: Self = Self {
		tag: TypeTag::Primitive,
		data: TypeData {
			primitive: ManuallyDrop::new(PrimitiveType::Float32),
		},
	};

	pub(crate) const PRIMITIVE_FLOAT64: Self = Self {
		tag: TypeTag::Primitive,
		data: TypeData {
			primitive: ManuallyDrop::new(PrimitiveType::Float64),
		},
	};

	// String and IName ////////////////////////////////////////////////////////

	pub(crate) const PRIMITIVE_STRING: Self = Self {
		tag: TypeTag::Primitive,
		data: TypeData {
			primitive: ManuallyDrop::new(PrimitiveType::String),
		},
	};

	pub(crate) const PRIMITIVE_INAME: Self = Self {
		tag: TypeTag::Primitive,
		data: TypeData {
			primitive: ManuallyDrop::new(PrimitiveType::IName),
		},
	};
}
