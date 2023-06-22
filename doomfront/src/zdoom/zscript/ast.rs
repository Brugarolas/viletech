//! Abstract syntax tree nodes.

mod actor;
mod expr;
mod lit;
mod structure;

use std::num::IntErrorKind;

use rowan::ast::AstNode;

use crate::{simple_astnode, zdoom};

use super::{Syn, SyntaxNode, SyntaxToken};

pub use self::{actor::*, expr::*, lit::*, structure::*};

/// A top-level element in a source file.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum TopLevel {
	ClassDef(ClassDef),
	ClassExtend(ClassExtend),
	ConstDef(ConstDef),
	EnumDef(EnumDef),
	MixinClassDef(MixinClassDef),
	Include(IncludeDirective),
	StructDef(StructDef),
	StructExtend(StructExtend),
	Version(VersionDirective),
}

impl AstNode for TopLevel {
	type Language = Syn;

	fn can_cast(kind: <Self::Language as rowan::Language>::Kind) -> bool
	where
		Self: Sized,
	{
		matches!(
			kind,
			Syn::ClassDef
				| Syn::ClassExtend
				| Syn::ConstDef | Syn::EnumDef
				| Syn::MixinClassDef
				| Syn::IncludeDirective
				| Syn::StructDef | Syn::StructExtend
				| Syn::VersionDirective
		)
	}

	fn cast(node: SyntaxNode) -> Option<Self>
	where
		Self: Sized,
	{
		match node.kind() {
			Syn::ClassDef => todo!(),
			Syn::ClassExtend => todo!(),
			Syn::ConstDef => todo!(),
			Syn::EnumDef => todo!(),
			Syn::MixinClassDef => todo!(),
			Syn::IncludeDirective => todo!(),
			Syn::StructDef => todo!(),
			Syn::StructExtend => todo!(),
			Syn::VersionDirective => todo!(),
			_ => None,
		}
	}

	fn syntax(&self) -> &SyntaxNode {
		match self {
			TopLevel::ClassDef(inner) => inner.syntax(),
			TopLevel::ClassExtend(inner) => inner.syntax(),
			TopLevel::ConstDef(inner) => inner.syntax(),
			TopLevel::EnumDef(inner) => inner.syntax(),
			TopLevel::MixinClassDef(inner) => inner.syntax(),
			TopLevel::Include(inner) => inner.syntax(),
			TopLevel::StructDef(inner) => inner.syntax(),
			TopLevel::StructExtend(inner) => inner.syntax(),
			TopLevel::Version(inner) => inner.syntax(),
		}
	}
}

// ConstDef ////////////////////////////////////////////////////////////////////

/// Wraps a node tagged [`Syn::ConstDef`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ConstDef(SyntaxNode);

simple_astnode!(Syn, ConstDef, Syn::ConstDef);

impl ConstDef {
	/// The returned token is always tagged [`Syn::Ident`].
	#[must_use]
	pub fn name(&self) -> SyntaxToken {
		self.0
			.children_with_tokens()
			.find_map(|elem| elem.into_token().filter(|token| token.kind() == Syn::Ident))
			.unwrap()
	}
}

// EnumDef /////////////////////////////////////////////////////////////////////

/// Wraps a node tagged [`Syn::EnuMDef`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct EnumDef(SyntaxNode);

simple_astnode!(Syn, EnumDef, Syn::EnumDef);

impl EnumDef {
	/// The returned token is always tagged [`Syn::Ident`].
	#[must_use]
	pub fn name(&self) -> SyntaxToken {
		self.0
			.children_with_tokens()
			.find_map(|elem| elem.into_token().filter(|token| token.kind() == Syn::Ident))
			.unwrap()
	}
}

// IncludeDirective ////////////////////////////////////////////////////////////

/// Wraps a node tagged [`Syn::IncludeDirective`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct IncludeDirective(SyntaxNode);

simple_astnode!(Syn, IncludeDirective, Syn::IncludeDirective);

// VersionDirective ////////////////////////////////////////////////////////////

/// Wraps a node tagged [`Syn::VersionDirective`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct VersionDirective(SyntaxNode);

simple_astnode!(Syn, VersionDirective, Syn::VersionDirective);

impl VersionDirective {
	pub fn version(&self) -> Result<zdoom::Version, IntErrorKind> {
		let lit = self.0.last_token().unwrap();
		debug_assert_eq!(lit.kind(), Syn::StringLit);
		let text = lit.text();
		let start = text.chars().position(|c| c == '"').unwrap();
		let end = text.chars().rev().position(|c| c == '"').unwrap();
		let span = (start + 1)..(text.len() - end - 1);
		let Some(content) = text.get(span) else { return Err(IntErrorKind::Empty); };
		content.parse()
	}
}

// IdentChain //////////////////////////////////////////////////////////////////

/// Wraps a node tagged [`Syn::IdentChain`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct IdentChain(pub(self) SyntaxNode);

simple_astnode!(Syn, IdentChain, Syn::IdentChain);

impl IdentChain {
	/// Each yielded token is tagged [`Syn::Ident`].
	pub fn parts(&self) -> impl Iterator<Item = SyntaxToken> {
		self.syntax()
			.children_with_tokens()
			.filter_map(|elem| elem.into_token().filter(|tok| tok.kind() == Syn::Ident))
	}
}
