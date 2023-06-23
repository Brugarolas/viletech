//! A general-purpose LL parser.
//!
//! This design is derived from those presented in the following articles:
//! - <https://matklad.github.io/2023/05/21/resilient-ll-parsing-tutorial.html>
//! - <https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html>

use std::cell::Cell;

use logos::Logos;
use rowan::{GreenNode, GreenNodeBuilder, SyntaxKind};

use crate::LangExt;

/// A general-purpose LL parser.
pub struct Parser<'i, L: LangExt> {
	source: &'i str,
	tokens: Vec<Lexeme<L>>,
	pos: usize,
	fuel: Cell<u32>,
	events: Vec<Event>,
	errors: Vec<Error<L>>,
}

impl<'i, L: LangExt> Parser<'i, L> {
	#[must_use]
	pub fn new(
		source: &'i str,
		extras: <<L as LangExt>::Token as logos::Logos<'i>>::Extras,
	) -> Self {
		Self {
			source,
			tokens: L::Token::lexer_with_extras(source, extras)
				.spanned()
				.map(|(result, span)| match result {
					Ok(t) => Lexeme { kind: t, span },
					Err(t) => Lexeme { kind: t, span },
				})
				.collect(),
			pos: 0,
			fuel: Cell::new(256),
			events: vec![],
			errors: vec![],
		}
	}

	/// Starts a new sub-tree. Also see [`Self::close`].
	#[must_use]
	pub fn open(&mut self) -> OpenMark {
		let checkpoint = OpenMark(self.events.len());
		self.events.push(Event::Open(L::kind_to_raw(L::ERR_NODE)));
		checkpoint
	}

	/// Also see [`Self::open`]. Will panic if no sub-trees are open.
	pub fn close(&mut self, mark: OpenMark, syn: L::Kind) -> CloseMark {
		self.events[mark.0] = Event::Open(L::kind_to_raw(syn));
		self.events.push(Event::Close);
		CloseMark(mark.0)
	}

	pub fn open_before(&mut self, mark: CloseMark) -> OpenMark {
		let ret = OpenMark(mark.0);
		self.events
			.insert(mark.0, Event::Open(L::kind_to_raw(L::ERR_NODE)));
		ret
	}

	/// Mind that this has to perform O(n) vector element shifting.
	pub fn cancel(&mut self, mark: OpenMark) {
		self.events.remove(mark.0);
	}

	pub fn advance(&mut self, syn: L::Kind) {
		assert!(!self.eof());
		self.fuel.set(256);
		self.events.push(Event::Advance(L::kind_to_raw(syn)));
		self.pos += 1;
	}

	pub fn advance_n(&mut self, syn: L::Kind, tokens: usize) {
		assert!(
			tokens >= 1,
			"`advance_n` was passed 0 at {:?} (`{}`)",
			self.current_span(),
			self.current_slice()
		);

		self.fuel.set(256);
		self.events
			.push(Event::AdvanceN(L::kind_to_raw(syn), tokens));
		self.pos += tokens;
	}

	#[must_use]
	pub fn eof(&self) -> bool {
		self.pos == self.tokens.len()
	}

	#[must_use]
	pub fn current_slice(&self) -> &str {
		&self.source[self.tokens[self.pos].span.clone()]
	}

	#[must_use]
	pub fn current_span(&self) -> logos::Span {
		self.tokens[self.pos].span.clone()
	}

	#[must_use]
	pub fn nth(&self, lookahead: usize) -> L::Token {
		if self.fuel.get() == 0 {
			panic!(
				"parser is not advancing (stuck at {:?})",
				self.tokens[self.pos].span
			)
		}

		self.fuel.set(self.fuel.get() - 1);

		self.tokens
			.get(self.pos + lookahead)
			.map_or(L::EOF, |lexeme| lexeme.kind)
	}

	/// Shorthand for `self.nth(0) == token`.
	#[must_use]
	pub fn at(&self, token: L::Token) -> bool {
		self.nth(0) == token
	}

	/// See [`Self::at`].
	#[must_use]
	pub fn at_any(&self, choices: &'static [L::Token]) -> bool {
		let token = self.nth(0);
		choices.iter().any(|t| *t == token)
	}

	/// Like [`Self::at`], but only matches `token` as long as it holds `string`,
	/// ASCII case-insensitively.
	///
	/// This is needed for (G)ZDoom DSLs, many of which are unspecified and use only an
	/// ad-hoc parser as DoomFront's reference implementation. Representing every niche
	/// keyword used by every one of these languages would add complexity to every parser
	/// (since each would have to treat foreign keywords as identifiers), so instead
	/// make the smaller languages look for their keywords through identifiers.
	#[must_use]
	pub fn at_str_nc(&self, token: L::Token, string: &'static str) -> bool {
		let eof = Lexeme {
			kind: L::EOF,
			span: self.source.len()..self.source.len(),
		};

		let lexeme = self.tokens.get(self.pos).unwrap_or(&eof);

		lexeme.kind == token && self.source[lexeme.span.clone()].eq_ignore_ascii_case(string)
	}

	/// See [`Self::at`].
	#[must_use]
	pub fn at_if(&self, predicate: fn(L::Token) -> bool) -> bool {
		predicate(self.nth(0))
	}

	/// If [`Self::at`] matches `token`, [`Self::advance`] with `syn`.
	pub fn eat(&mut self, token: L::Token, syn: L::Kind) -> bool {
		if self.at(token) {
			self.advance(syn);
			true
		} else {
			false
		}
	}

	/// Like [`Self::eat`] but checks [`Self::at`] on each choice in the given order.
	pub fn eat_any(&mut self, choices: &'static [(L::Token, L::Kind)]) -> bool {
		for choice in choices {
			if self.at(choice.0) {
				self.advance(choice.1);
				return true;
			}
		}

		false
	}

	/// Composes [`Self::eat`] and [`Self::at_str_nc`].
	#[must_use]
	pub fn eat_str_nc(&mut self, token: L::Token, string: &'static str, syn: L::Kind) -> bool {
		if self.at_str_nc(token, string) {
			self.advance(syn);
			return true;
		}

		false
	}

	/// Composes [`Self::eat`] and [`Self::at_if`].
	#[must_use]
	pub fn eat_if(&mut self, predicate: fn(L::Token) -> bool, syn: L::Kind) -> bool {
		if self.at_if(predicate) {
			self.advance(syn);
			true
		} else {
			false
		}
	}

	/// If [`Self::eat`] fails to consume `token`, raise an error.
	pub fn expect(&mut self, token: L::Token, syn: L::Kind, expected: &'static [&'static str]) {
		if self.eat(token, syn) {
			return;
		}

		self.raise(expected);
	}

	/// If [`Self::eat_str_nc`] fails to consume `token` corresponding to `string`
	/// ASCII-case insensitively, raise an error.
	pub fn expect_str_nc(
		&mut self,
		token: L::Token,
		string: &'static str,
		syn: L::Kind,
		expected: &'static [&'static str],
	) {
		if self.eat_str_nc(token, string, syn) {
			return;
		}

		self.raise(expected);
	}

	/// Composes [`Self::expect`] and [`Self::eat_if`].
	pub fn expect_if(
		&mut self,
		predicate: fn(L::Token) -> bool,
		syn: L::Kind,
		expected: &'static [&'static str],
	) {
		if self.eat_if(predicate, syn) {
			return;
		}

		self.raise(expected);
	}

	/// Composes [`Self::expect`] and [`Self::eat_any`].
	pub fn expect_any(
		&mut self,
		choices: &'static [(L::Token, L::Kind)],
		expected: &'static [&'static str],
	) {
		for choice in choices {
			if self.eat(choice.0, choice.1) {
				return;
			}
		}

		self.raise(expected);
	}

	/// Composes [`Self::expect_any`] and [`Self::expect_str_nc`].
	pub fn expect_any_str_nc(
		&mut self,
		choices: &'static [(L::Token, &'static str, L::Kind)],
		expected: &'static [&'static str],
	) {
		for choice in choices {
			if self.eat_str_nc(choice.0, choice.1, choice.2) {
				return;
			}
		}

		self.raise(expected);
	}

	/// Finds the next token (which may be [`L::EOF`]) for which `predicate`
	/// returns `true`.
	pub fn next_filtered(&self, predicate: fn(L::Token) -> bool) -> L::Token {
		self.tokens[self.pos..]
			.iter()
			.find_map(|t| {
				if predicate(t.kind) {
					Some(t.kind)
				} else {
					None
				}
			})
			.unwrap_or(L::EOF)
	}

	/// Like [`Self::next_filtered`] but skips ahead of the current token if possible.
	pub fn lookahead_filtered(&self, predicate: fn(L::Token) -> bool) -> L::Token {
		if self.pos >= self.tokens.len() {
			return L::EOF;
		}

		self.tokens[(self.pos + 1)..]
			.iter()
			.find_map(|t| {
				if predicate(t.kind) {
					Some(t.kind)
				} else {
					None
				}
			})
			.unwrap_or(L::EOF)
	}

	fn raise(&mut self, expected: &'static [&'static str]) {
		self.errors.push(Error {
			expected,
			found: self.tokens.get(self.pos).cloned().unwrap_or(Lexeme {
				kind: L::EOF,
				span: self.source.len()..self.source.len(),
			}),
		});
	}

	/// [Opens](Self::open) a new error node, [advances](Self::advance) it with
	/// a `syn` token, and then [closes](Self::close) it.
	pub fn advance_with_error(&mut self, syn: L::Kind, expected: &'static [&'static str]) {
		let ckpt = self.open();
		self.raise(expected);
		self.advance(syn);
		self.close(ckpt, L::ERR_NODE);
	}

	/// Raise an error and advance the token cursor (if not at the end-of-input).
	/// The sub-tree opened by `checkpoint` gets [closed](Self::close) with `err`.
	pub fn advance_err_and_close(
		&mut self,
		checkpoint: OpenMark,
		token: L::Kind,
		err: L::Kind,
		expected: &'static [&'static str],
	) -> CloseMark {
		self.raise(expected);

		if !self.eof() {
			self.advance(token);
		}

		self.close(checkpoint, err)
	}

	/// Use when getting ready to open a new node to validate that the parser
	/// is currently at the first expected token of that node.
	pub fn debug_assert_at(&self, token: L::Token)
	where
		L::Token: std::fmt::Debug,
	{
		debug_assert!(
			self.at(token),
			"parser expected to be at a `{token:#?}`, but is at `{t:#?}`\r\n\
			(position {pos}, span {span:?}, slice: `{slice}`)",
			t = self.nth(0),
			pos = self.pos,
			span = self.current_span(),
			slice = self.current_slice()
		);
	}

	/// See [`Self::debug_assert_at`].
	pub fn debug_assert_at_any(&self, choices: &'static [L::Token])
	where
		L::Token: std::fmt::Debug,
	{
		debug_assert!(
			self.at_any(choices),
			"parser's current token did not pass a predicate (at `{t:#?}`)\r\n\
			(position {pos}, span {span:?}, slice: `{slice}`)",
			t = self.nth(0),
			pos = self.pos,
			span = self.current_span(),
			slice = self.current_slice()
		);
	}

	/// See [`Self::debug_assert_at`].
	pub fn debug_assert_at_if(&self, predicate: fn(L::Token) -> bool)
	where
		L::Token: std::fmt::Debug,
	{
		debug_assert!(
			predicate(self.nth(0)),
			"parser's current token did not pass a predicate (at `{t:#?}`)\r\n\
			(position {pos}, span {span:?}, slice: `{slice}`)",
			t = self.nth(0),
			pos = self.pos,
			span = self.current_span(),
			slice = self.current_slice()
		);
	}

	/// Panics if an [opened] subtree was never [closed], or if no sub-trees
	/// were ever opened at all.
	///
	/// [opened]: Self::open
	/// [closed]: Self::close
	#[must_use]
	pub fn finish(self) -> (GreenNode, Vec<Error<L>>) {
		let mut tokens = self.tokens.into_iter();
		let mut builder = GreenNodeBuilder::new();

		for event in self.events {
			match event {
				Event::Advance(syn) => {
					let lexeme = tokens.next().unwrap();
					builder.token(syn, &self.source[lexeme.span]);
				}
				Event::Open(syn) => {
					builder.start_node(syn);
				}
				Event::Close => {
					builder.finish_node();
				}
				Event::AdvanceN(syn, 1) => {
					let lexeme = tokens.next().unwrap();
					builder.token(syn, &self.source[lexeme.span]);
				}
				Event::AdvanceN(syn, n) => {
					let start = tokens.next().unwrap().span.start;

					// An unconditional assertion in `Self::advance_n` protects
					// against `n` ever being less than one.
					for _ in 0..(n - 1) {
						let _ = tokens.next().unwrap();
					}

					let end = tokens.next().unwrap().span.end;
					builder.token(syn, &self.source[start..end]);
				}
			}
		}

		assert!(tokens.next().is_none(), "not all tokens were consumed");
		(builder.finish(), self.errors)
	}
}

/// See [`Parser::open`] and [`Parser::close`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpenMark(usize);

/// See [`Parser::close`] and [`Parser::open_before`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CloseMark(usize);

pub struct Error<L: LangExt> {
	expected: &'static [&'static str],
	found: Lexeme<L>,
}

impl<L: LangExt> Error<L> {
	#[must_use]
	pub fn expected(&self) -> &'static [&'static str] {
		self.expected
	}

	#[must_use]
	pub fn found(&self) -> Lexeme<L> {
		self.found.clone()
	}
}

impl<L: LangExt> std::fmt::Display for Error<L>
where
	L::Token: std::fmt::Display,
{
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(
			f,
			"found {} at {:?} - expected one of the following: {}",
			self.found.kind,
			self.found.span,
			{
				let mut out = String::new();

				for e in self.expected {
					out.push_str(e);
					out.push('/');
				}

				out.pop();
				out
			}
		)
	}
}

impl<L: LangExt> std::fmt::Debug for Error<L>
where
	L::Token: std::fmt::Debug,
{
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Error")
			.field("expected", &self.expected)
			.field("found", &self.found)
			.finish()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Lexeme<L: LangExt> {
	kind: L::Token,
	span: logos::Span,
}

#[derive(Debug)]
enum Event {
	Open(SyntaxKind),
	Close,
	Advance(SyntaxKind),
	AdvanceN(SyntaxKind, usize),
}