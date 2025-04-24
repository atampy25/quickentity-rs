#![feature(proc_macro_quote)]
#![feature(proc_macro_span)]

use proc_macro::{quote, Delimiter, Group, Literal, TokenStream, TokenTree};

static RESERVED_KEYWORDS: [&str; 57] = [
	"as",
	"use",
	"extern crate",
	"break",
	"const",
	"continue",
	"crate",
	"else",
	"if",
	"enum",
	"extern",
	"false",
	"fn",
	"for",
	"if",
	"impl",
	"in",
	"for",
	"let",
	"loop",
	"match",
	"mod",
	"move",
	"mut",
	"pub",
	"impl",
	"ref",
	"return",
	"Self",
	"self",
	"static",
	"struct",
	"super",
	"trait",
	"true",
	"type",
	"unsafe",
	"use",
	"where",
	"while",
	"abstract",
	"alignof",
	"become",
	"box",
	"do",
	"final",
	"macro",
	"offsetof",
	"override",
	"priv",
	"proc",
	"pure",
	"sizeof",
	"typeof",
	"unsized",
	"virtual",
	"yield"
];

/// Allows for a postfix `.ctx` which generates a marginally helpful `.context` call with the prior function calls and source file and line number. Like `unwrap` but for `Result`-based error propagation.
#[proc_macro_attribute]
pub fn auto_context(_: TokenStream, item: TokenStream) -> TokenStream {
	let mut tokens = TokenStream::new();

	let mut prev = [None, None, None, None, None, None, None, None, None, None, None, None];
	for x in item {
		tokens.extend(process_tokentree(prev.clone(), x.clone()));
		prev.rotate_left(1);
		prev[11] = Some(x);
	}

	tokens
}

fn process_tokentree(prev: [Option<TokenTree>; 12], tree: TokenTree) -> Vec<TokenTree> {
	match tree {
		TokenTree::Group(x) => vec![TokenTree::Group({
			let mut group = Group::new(x.delimiter(), {
				let mut tokens = TokenStream::new();

				let mut prev = [None, None, None, None, None, None, None, None, None, None, None, None];
				for x in x.stream() {
					tokens.extend(process_tokentree(prev.clone(), x.clone()));
					prev.rotate_left(1);
					prev[11] = Some(x);
				}

				tokens
			});

			group.set_span(x.span()); // avoid losing span information

			group
		})],

		TokenTree::Ident(x) => {
			if let Some(TokenTree::Punct(punct)) = &prev[11] {
				if let Some(TokenTree::Ident(func_name)) = &prev[9] {
					if let Some(TokenTree::Group(func_args)) = &prev[10] {
						if punct.as_char() == '.'
							&& func_args.delimiter() == Delimiter::Parenthesis
							&& x.to_string() == "ctx"
						{
							let mut context_tokens = vec![];

							for token_ind in (0..11).rev() {
								if let Some(token) = &prev[token_ind] {
									if !matches!(token, TokenTree::Punct(x) if x.as_char() != '.' && x.as_char() != '?')
										&& !matches!(token, TokenTree::Ident(x) if RESERVED_KEYWORDS.contains(&x.to_string().as_str()))
										&& !matches!(token, TokenTree::Group(x) if x.delimiter() == Delimiter::Brace)
									{
										context_tokens.push(token.clone());
									} else {
										break;
									}
								}
							}

							let msg = TokenTree::Literal(Literal::string(&format!(
								"{} at {}:{}",
								context_tokens
									.into_iter()
									.rev()
									.map(|x| stringify(x))
									.collect::<Vec<_>>()
									.join(""),
								func_name
									.span()
									.file(),
								func_name.span().start().line()
							)));

							quote!(context($msg)).into_iter().collect()
						} else {
							vec![TokenTree::Ident(x)]
						}
					} else {
						vec![TokenTree::Ident(x)]
					}
				} else if let Some(TokenTree::Ident(func_name)) = &prev[10] {
					if punct.as_char() == '.'
						&& x.to_string() == "ctx"
					{
						let mut context_tokens = vec![];

						for token_ind in (0..11).rev() {
							if let Some(token) = &prev[token_ind] {
								if !matches!(token, TokenTree::Punct(x) if x.as_char() != '.' && x.as_char() != '?')
									&& !matches!(token, TokenTree::Ident(x) if RESERVED_KEYWORDS.contains(&x.to_string().as_str()))
									&& !matches!(token, TokenTree::Group(x) if x.delimiter() == Delimiter::Brace)
								{
									context_tokens.push(token.clone());
								} else {
									break;
								}
							}
						}

						let msg = TokenTree::Literal(Literal::string(&format!(
							"{} at {}:{}",
							context_tokens
								.into_iter()
								.rev()
								.map(|x| stringify(x))
								.collect::<Vec<_>>()
								.join(""),
							func_name
								.span()
								.file(),
							func_name.span().start().line()
						)));

						quote!(context($msg)).into_iter().collect()
					} else {
						vec![TokenTree::Ident(x)]
					}
				} else {
					vec![TokenTree::Ident(x)]
				}
			} else {
				vec![TokenTree::Ident(x)]
			}
		}
		TokenTree::Literal(x) => vec![TokenTree::Literal(x)],
		TokenTree::Punct(x) => vec![TokenTree::Punct(x)]
	}
}

fn stringify(item: TokenTree) -> String {
	match item {
		TokenTree::Group(x) => x.to_string(),
		TokenTree::Ident(x) => x.to_string(),
		TokenTree::Literal(x) => x.to_string(),
		TokenTree::Punct(x) => x.to_string()
	}
}
