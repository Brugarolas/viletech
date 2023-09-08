//! Abstract syntax tree nodes.

mod actor;
mod expr;
mod stat;
mod structure;
mod types;

use std::num::IntErrorKind;

use rowan::ast::AstNode;

use crate::{
	simple_astnode,
	zdoom::{self, ast::LitToken},
	AstError, AstResult,
};

use super::{Syn, SyntaxNode, SyntaxToken};

pub use self::{actor::*, expr::*, stat::*, structure::*, types::*};

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
			Syn::ClassDef => Some(Self::ClassDef(ClassDef(node))),
			Syn::ClassExtend => Some(Self::ClassExtend(ClassExtend(node))),
			Syn::ConstDef => Some(Self::ConstDef(ConstDef(node))),
			Syn::EnumDef => Some(Self::EnumDef(EnumDef(node))),
			Syn::MixinClassDef => Some(Self::MixinClassDef(MixinClassDef(node))),
			Syn::IncludeDirective => Some(Self::Include(IncludeDirective(node))),
			Syn::StructDef => Some(Self::StructDef(StructDef(node))),
			Syn::StructExtend => Some(Self::StructExtend(StructExtend(node))),
			Syn::VersionDirective => Some(Self::Version(VersionDirective(node))),
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
	pub fn name(&self) -> AstResult<SyntaxToken> {
		self.0
			.children_with_tokens()
			.find_map(|elem| elem.into_token().filter(|token| token.kind() == Syn::Ident))
			.ok_or(AstError::Missing)
	}

	pub fn initializer(&self) -> AstResult<Expr> {
		match self.0.last_child() {
			Some(node) => Expr::cast(node).ok_or(AstError::Incorrect),
			None => Err(AstError::Missing),
		}
	}

	pub fn docs(&self) -> impl Iterator<Item = DocComment> {
		doc_comments(&self.0)
	}
}

// EnumDef /////////////////////////////////////////////////////////////////////

/// Wraps a node tagged [`Syn::EnumDef`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct EnumDef(SyntaxNode);

simple_astnode!(Syn, EnumDef, Syn::EnumDef);

impl EnumDef {
	/// The returned token is always tagged [`Syn::Ident`].
	pub fn name(&self) -> AstResult<SyntaxToken> {
		self.0
			.children_with_tokens()
			.find_map(|elem| elem.into_token().filter(|token| token.kind() == Syn::Ident))
			.ok_or(AstError::Missing)
	}

	#[must_use]
	pub fn type_spec(&self) -> Option<(SyntaxToken, EnumType)> {
		self.0.children_with_tokens().find_map(|elem| {
			let ret1 = match elem.kind() {
				Syn::KwSByte => EnumType::KwSByte,
				Syn::KwByte => EnumType::KwByte,
				Syn::KwInt8 => EnumType::KwInt8,
				Syn::KwUInt8 => EnumType::KwUInt8,
				Syn::KwShort => EnumType::KwShort,
				Syn::KwUShort => EnumType::KwUShort,
				Syn::KwInt16 => EnumType::KwInt16,
				Syn::KwUInt16 => EnumType::KwUInt16,
				Syn::KwInt => EnumType::KwInt,
				Syn::KwUInt => EnumType::KwUInt,
				_ => return None,
			};

			Some((elem.into_token().unwrap(), ret1))
		})
	}

	pub fn variants(&self) -> impl Iterator<Item = EnumVariant> {
		self.0.children().filter_map(EnumVariant::cast)
	}

	pub fn docs(&self) -> impl Iterator<Item = DocComment> {
		doc_comments(&self.0)
	}
}

/// See [`EnumDef::type_spec`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum EnumType {
	KwSByte,
	KwByte,
	KwInt8,
	KwUInt8,
	KwShort,
	KwUShort,
	KwInt16,
	KwUInt16,
	KwInt,
	KwUInt,
}

impl std::fmt::Display for EnumType {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::KwSByte => write!(f, "sbyte"),
			Self::KwByte => write!(f, "byte"),
			Self::KwInt8 => write!(f, "int8"),
			Self::KwUInt8 => write!(f, "uint8"),
			Self::KwShort => write!(f, "short"),
			Self::KwUShort => write!(f, "ushort"),
			Self::KwInt16 => write!(f, "int16"),
			Self::KwUInt16 => write!(f, "uint16"),
			Self::KwInt => write!(f, "int"),
			Self::KwUInt => write!(f, "uint"),
		}
	}
}

/// Wraps a node tagged [`Syn::EnumVariant`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct EnumVariant(SyntaxNode);

simple_astnode!(Syn, EnumVariant, Syn::EnumVariant);

impl EnumVariant {
	/// The returned token is always tagged [`Syn::Ident`].
	#[must_use]
	pub fn name(&self) -> SyntaxToken {
		self.0.first_token().unwrap()
	}

	#[must_use]
	pub fn initializer(&self) -> Option<Expr> {
		self.0.last_child().map(|node| Expr::cast(node).unwrap())
	}

	pub fn docs(&self) -> impl Iterator<Item = DocComment> {
		doc_comments(&self.0)
	}
}

// IncludeDirective ////////////////////////////////////////////////////////////

/// Wraps a node tagged [`Syn::IncludeDirective`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct IncludeDirective(SyntaxNode);

simple_astnode!(Syn, IncludeDirective, Syn::IncludeDirective);

impl IncludeDirective {
	/// The returned token is always tagged [`Syn::StringLit`].
	pub fn argument(&self) -> AstResult<LitToken<Syn>> {
		self.0
			.last_token()
			.filter(|token| token.kind() == Syn::StringLit)
			.map(LitToken::new)
			.ok_or(AstError::Missing)
	}
}

// VersionDirective ////////////////////////////////////////////////////////////

/// Wraps a node tagged [`Syn::VersionDirective`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct VersionDirective(SyntaxNode);

simple_astnode!(Syn, VersionDirective, Syn::VersionDirective);

impl VersionDirective {
	/// [`IntErrorKind::Empty`] is returned if the expected string literal is absent.
	pub fn version(&self) -> Result<zdoom::Version, IntErrorKind> {
		let lit = self.0.last_token().ok_or(IntErrorKind::Empty)?;
		let Syn::StringLit = lit.kind() else { return Err(IntErrorKind::Empty) };
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

// DeprecationQual /////////////////////////////////////////////////////////////

/// Wraps a node tagged [`Syn::DeprecationQual`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DeprecationQual(SyntaxNode);

simple_astnode!(Syn, DeprecationQual, Syn::DeprecationQual);

impl DeprecationQual {
	/// The returned token is always tagged [`Syn::StringLit`].
	pub fn version(&self) -> AstResult<LitToken<Syn>> {
		self.0
			.children_with_tokens()
			.find_map(|elem| {
				elem.into_token()
					.filter(|token| token.kind() == Syn::StringLit)
					.map(LitToken::new)
			})
			.ok_or(AstError::Missing)
	}

	/// The returned token is always tagged [`Syn::StringLit`].
	#[must_use]
	pub fn message(&self) -> Option<SyntaxToken> {
		self.0
			.children_with_tokens()
			.filter_map(|elem| elem.into_token())
			.skip_while(|token| token.kind() != Syn::Comma)
			.find_map(|token| (token.kind() == Syn::StringLit).then_some(token))
	}
}

// VersionQual /////////////////////////////////////////////////////////////////

/// Wraps a node tagged [`Syn::VersionQual`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct VersionQual(SyntaxNode);

simple_astnode!(Syn, VersionQual, Syn::VersionQual);

impl VersionQual {
	/// The returned token is always tagged [`Syn::StringLit`].
	pub fn version(&self) -> AstResult<LitToken<Syn>> {
		self.0
			.children_with_tokens()
			.skip_while(|elem| elem.kind() != Syn::ParenL)
			.find_map(|elem| {
				elem.into_token()
					.filter(|token| token.kind() == Syn::StringLit)
					.map(LitToken::new)
			})
			.ok_or(AstError::Missing)
	}
}

// LocalVar ////////////////////////////////////////////////////////////////////

/// Wraps a node tagged [`Syn::LocalVar`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct LocalVar(pub(self) SyntaxNode);

simple_astnode!(Syn, LocalVar, Syn::LocalVar);

impl LocalVar {
	pub fn type_ref(&self) -> AstResult<TypeRef> {
		let Some(node) = self.0.first_child() else { return Err(AstError::Missing); };
		TypeRef::cast(node).ok_or(AstError::Incorrect)
	}

	pub fn initializers(&self) -> impl Iterator<Item = LocalVarInit> {
		self.0.children().filter_map(LocalVarInit::cast)
	}
}

/// Wraps a node tagged [`Syn::LocalVarInit`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct LocalVarInit(SyntaxNode);

simple_astnode!(Syn, LocalVarInit, Syn::LocalVarInit);

impl LocalVarInit {
	/// The returned token is always tagged [`Syn::Ident`].
	pub fn name(&self) -> AstResult<SyntaxToken> {
		let Some(token) = self.0.first_token() else { return Err(AstError::Missing); };

		match token.kind() {
			Syn::Ident => Ok(token),
			_ => Err(AstError::Incorrect),
		}
	}

	#[must_use]
	pub fn array_len(&self) -> Option<ArrayLen> {
		let Some(node) = self.0.first_child() else { return None; };
		ArrayLen::cast(node)
	}

	#[must_use]
	pub fn single_init(&self) -> Option<Expr> {
		let Some(last) = self.0.last_token() else { return None; };

		if last.kind() == Syn::BraceR {
			return None;
		}

		let Some(node) = self.0.last_child() else { return None; };
		Expr::cast(node)
	}

	#[must_use]
	pub fn braced_inits(&self) -> Option<impl Iterator<Item = Expr>> {
		let Some(last) = self.0.last_token() else { return None; };

		if last.kind() != Syn::BraceR {
			return None;
		}

		Some(self.0.children().filter_map(Expr::cast))
	}
}

// VarName /////////////////////////////////////////////////////////////////////

/// Wraps a node tagged [`Syn::VarName`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct VarName(pub(self) SyntaxNode);

simple_astnode!(Syn, VarName, Syn::VarName);

impl VarName {
	/// The returned token is always tagged [`Syn::Ident`].
	#[must_use]
	pub fn ident(&self) -> SyntaxToken {
		let ret = self.0.first_token().unwrap();
		debug_assert_eq!(ret.kind(), Syn::Ident);
		ret
	}

	pub fn array_lengths(&self) -> impl Iterator<Item = ArrayLen> {
		self.0.children().map(|node| {
			debug_assert_eq!(node.kind(), Syn::ArrayLen);
			ArrayLen(node)
		})
	}
}

// ArrayLen ////////////////////////////////////////////////////////////////////

/// Wraps a node tagged [`Syn::ArrayLen`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ArrayLen(pub(self) SyntaxNode);

simple_astnode!(Syn, ArrayLen, Syn::ArrayLen);

impl ArrayLen {
	#[must_use]
	pub fn expr(&self) -> Option<Expr> {
		self.0.first_child().map(|node| Expr::cast(node).unwrap())
	}
}

// DocComment //////////////////////////////////////////////////////////////////

/// Wraps a [`Syn::DocComment`] token. Provides a convenience function for
/// stripping preceding slashes and surrounding whitespace.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DocComment(SyntaxToken);

impl DocComment {
	#[must_use]
	pub fn text_trimmed(&self) -> &str {
		self.0.text().trim_matches('/').trim()
	}
}

impl std::ops::Deref for DocComment {
	type Target = SyntaxToken;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

// Common AST helper functions /////////////////////////////////////////////////

pub(self) fn doc_comments(node: &SyntaxNode) -> impl Iterator<Item = DocComment> {
	node.children_with_tokens()
		.take_while(|elem| elem.kind() == Syn::DocComment)
		.filter_map(|elem| {
			elem.into_token()
				.filter(|token| token.kind() == Syn::DocComment)
				.map(DocComment)
		})
}
