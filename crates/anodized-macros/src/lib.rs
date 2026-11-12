#![doc = include_str!("../README.md")]

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::ToTokens;
use syn::{Expr, Item, TraitItemFn, parse_macro_input};

use anodized_core::{
    annotate::Specified as _,
    instrument::{Mode, RawCfg, fns::make_try_call, make_item_error},
    syntax::SpecFields,
};

const CONFIG: Mode = Mode::from_raw_cfg(RawCfg {
    anodized_discard_specs: cfg!(anodized_discard_specs),
    anodized_embed_specs: cfg!(anodized_embed_specs),
    anodized_charon: cfg!(anodized_charon),
    anodized_hax: cfg!(anodized_hax),
    anodized_panic: cfg!(anodized_panic),
    anodized_print: cfg!(anodized_print),
    anodized_try: cfg!(anodized_try),
});

/// Attaches a specification to supported program elements.
///
/// Compiler `cfg` settings select how the specification is processed:
/// - `anodized_discard_specs` discards it after parsing.
/// - `anodized_embed_specs` encodes it as hidden items for static analysis.
/// - `anodized_charon` selects static encoding with Charon contract attributes.
/// - `anodized_hax` selects static encoding with Hax attributes.
/// - Otherwise, dynamic mode validates specification expressions and adds runtime checks
///   controlled by `anodized_panic`, `anodized_print`, and `anodized_try`.
///
/// With no settings, runtime failure actions are disabled, but captures still execute.
#[proc_macro_attribute]
pub fn spec(args: TokenStream, input: TokenStream) -> TokenStream {
    // Parse the `#[spec]` attribute's arguments.
    let spec_fields = parse_macro_input!(args as SpecFields);

    // Parse the item to which the attribute is attached. A function whose
    // body holds no `#[spec]` keeps its body as its tokens: the
    // instrumentation wraps the body and reads none of it.
    let input = proc_macro2::TokenStream::from(input);
    let item = match verbatim_fn(&input) {
        Some(func) => Item::Fn(func),
        None => match syn::parse2::<Item>(input) {
            Ok(item) => item,
            Err(e) => return e.to_compile_error().into(),
        },
    };

    let result = match item {
        Item::Fn(mut func) => func
            .parse_spec_from_fields(spec_fields)
            .and_then(|spec| CONFIG.instrument_item_fn(spec, func)),
        Item::Trait(mut the_trait) => the_trait
            .parse_spec_from_fields(spec_fields)
            .and_then(|spec| CONFIG.instrument_item_trait(spec, the_trait)),
        Item::Impl(mut the_impl) if the_impl.trait_.is_some() => the_impl
            .parse_spec_from_fields(spec_fields)
            .and_then(|spec| CONFIG.instrument_item_trait_impl(spec, the_impl)),
        Item::Impl(mut the_impl) if the_impl.trait_.is_none() => the_impl
            .parse_spec_from_fields(spec_fields)
            .and_then(|spec| CONFIG.instrument_item_impl(spec, the_impl)),
        Item::Const(_) => Err(make_item_error(&item, "const")),
        Item::Enum(mut the_enum) => the_enum
            .parse_spec_from_fields(spec_fields)
            .and_then(|spec| CONFIG.instrument_item_enum(spec, the_enum)),
        Item::ExternCrate(_) => Err(make_item_error(&item, "extern crate")),
        Item::ForeignMod(_) => Err(make_item_error(&item, "extern block")),
        Item::Macro(_) => Err(make_item_error(&item, "macro")),
        Item::Mod(_) => Err(make_item_error(&item, "mod")),
        Item::Static(_) => Err(make_item_error(&item, "static")),
        Item::Struct(mut the_struct) => the_struct
            .parse_spec_from_fields(spec_fields)
            .and_then(|spec| CONFIG.instrument_item_struct(spec, the_struct)),
        Item::TraitAlias(_) => Err(make_item_error(&item, "trait alias")),
        Item::Type(_) => Err(make_item_error(&item, "type")),
        Item::Union(_) => Err(make_item_error(&item, "union")),
        Item::Use(_) => Err(make_item_error(&item, "use")),
        Item::Verbatim(ref tokens) => {
            // Try to parse as a trait fn
            if let Ok(trait_fn) = syn::parse2::<TraitItemFn>(tokens.clone()) {
                Err(syn::Error::new_spanned(
                    &trait_fn,
                    r#"The enclosing trait must have a `#[spec]` annotation."#,
                ))
            } else {
                Err(make_item_error(&item, "<unexpected>"))
            }
        }
        _ => Err(make_item_error(&item, "<unknown>")),
    };

    match result {
        Ok(item) => item.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// The attributes, the visibility and the signature of a function.
struct FnHead(Vec<syn::Attribute>, syn::Visibility, syn::Signature);

impl syn::parse::Parse for FnHead {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(Self(
            input.call(syn::Attribute::parse_outer)?,
            input.parse()?,
            input.parse()?,
        ))
    }
}

/// `input` as a function whose body is one verbatim statement, when it is a
/// function that is not `const` and whose body holds no `#[spec]`.
fn verbatim_fn(input: &proc_macro2::TokenStream) -> Option<syn::ItemFn> {
    use proc_macro2::{Delimiter, TokenTree};
    let mut head: Vec<TokenTree> = input.clone().into_iter().collect();
    let Some(TokenTree::Group(body)) = head.pop() else {
        return None;
    };
    if body.delimiter() != Delimiter::Brace {
        return None;
    }
    // A `const fn` reads the `return`s of its body, so it takes the whole parse.
    let keyword = head.iter().find_map(|tree| match tree {
        TokenTree::Ident(ident) if ident == "fn" || ident == "const" => Some(ident.to_string()),
        _ => None,
    })?;
    if keyword != "fn" || holds_spec(body.stream()) {
        return None;
    }
    let FnHead(attrs, vis, sig) = syn::parse2(head.into_iter().collect()).ok()?;
    Some(syn::ItemFn {
        attrs,
        vis,
        sig,
        block: Box::new(syn::Block {
            brace_token: syn::token::Brace {
                span: body.delim_span(),
            },
            stmts: vec![syn::Stmt::Expr(syn::Expr::Verbatim(body.stream()), None)],
        }),
    })
}

/// Whether `tokens` hold an attribute `#[spec …]` at any depth.
fn holds_spec(tokens: proc_macro2::TokenStream) -> bool {
    use proc_macro2::{Delimiter, TokenTree};
    let mut after_pound = false;
    for tree in tokens {
        match tree {
            TokenTree::Punct(punct) => after_pound = punct.as_char() == '#',
            TokenTree::Group(group) => {
                if after_pound
                    && group.delimiter() == Delimiter::Bracket
                    && matches!(group.stream().into_iter().next(), Some(TokenTree::Ident(ident)) if ident == "spec")
                {
                    return true;
                }
                if holds_spec(group.stream()) {
                    return true;
                }
                after_pound = false;
            }
            TokenTree::Ident(_) | TokenTree::Literal(_) => after_pound = false,
        }
    }
    false
}

#[proc_macro]
pub fn try_call(args: TokenStream) -> TokenStream {
    if !CONFIG.emits_try_fn() {
        return syn::Error::new(
            Span::call_site(),
            "`try_call` needs the `anodized_try` build `cfg` to be enabled",
        )
        .to_compile_error()
        .into();
    }

    let expr = parse_macro_input!(args as Expr);

    match make_try_call(expr) {
        Ok(call) => call.into_token_stream().into(),
        Err(error) => error.to_compile_error().into(),
    }
}

/// Control enforcement of the type's spec (a.k.a. refinement). May be applied to a type in a `fn`
/// signature, or a `struct` or `enum` definition. The enclosing item **must** have a `#[spec]`
/// attribute.
///
/// - `Spec!(T)`:
///     - The input's type spec holds on entry.
///     - The output's type spec holds on exit.
///     - The field's type spec holds in a structurally recursive way.
/// - `Spec!(T, out)`: The input's type spec holds only on exit (not on entry).
/// - `Spec!(T, inout)`: The input's type spec holds on both entry and exit.
///
/// This macro exists *only* to carry this documentation. It should not be invoked, because
/// `anodized-core` removes it as part of processing `#[spec]` attributes.
#[proc_macro]
#[allow(non_snake_case)]
pub fn Spec(_: TokenStream) -> TokenStream {
    syn::Error::new(
        Span::call_site(),
        "`Spec!` must appear inside an item annotated with `#[spec]`",
    )
    .to_compile_error()
    .into()
}

#[cfg(test)]
mod tests {
    use anodized_core::annotate::Specified as _;
    use quote::quote;

    use super::{CONFIG, SpecFields, holds_spec, verbatim_fn};

    /// The expansion of `item` under `spec`, with its body verbatim or
    /// parsed whole.
    fn expand(
        spec: &proc_macro2::TokenStream,
        item: &proc_macro2::TokenStream,
        verbatim: bool,
    ) -> String {
        let fields: SpecFields = syn::parse2(spec.clone()).unwrap();
        let func = if verbatim {
            verbatim_fn(item).expect("a plain function keeps its body verbatim")
        } else {
            syn::parse2::<syn::ItemFn>(item.clone()).unwrap()
        };
        let mut func = func;
        let spec = func.parse_spec_from_fields(fields).unwrap();
        CONFIG.instrument_item_fn(spec, func).unwrap().to_string()
    }

    #[test]
    fn a_verbatim_body_expands_as_the_parsed_body() {
        let cases = [
            (
                quote! { requires: x > 0, captures: old = x, ensures: |out| *out >= old },
                quote! {
                    #[inline]
                    pub fn f(x: u32) -> u32 {
                        if x > 3 { return x; }
                        let y = x + 1;
                        y
                    }
                },
            ),
            (
                quote! { ensures: self.n > 0 },
                quote! {
                    fn g<T: Clone, const N: usize>(&mut self, items: [T; N]) where T: Default {
                        for item in items { let _ = item.clone(); }
                        self.n += 1;
                    }
                },
            ),
            (
                quote! { requires: x.is_finite() },
                quote! { async fn h(x: f32) -> f32 { x / 2.0 } },
            ),
        ];
        for (spec, item) in &cases {
            assert_eq!(expand(spec, item, true), expand(spec, item, false));
        }
    }

    #[test]
    fn a_const_fn_a_loop_spec_or_no_body_takes_the_whole_parse() {
        assert!(verbatim_fn(&quote! { const fn f() -> u32 { 1 } }).is_none());
        assert!(verbatim_fn(&quote! { pub const unsafe fn f() {} }).is_none());
        assert!(
            verbatim_fn(&quote! { fn f() { #[spec(maintains: true)] while false {} } }).is_none()
        );
        assert!(verbatim_fn(&quote! { fn f(&self); }).is_none());
        assert!(verbatim_fn(&quote! { pub fn f<const N: usize>() -> usize { N } }).is_some());
    }

    #[test]
    fn a_spec_attribute_is_found_at_any_depth() {
        assert!(holds_spec(
            quote! { { if c { #[spec(maintains: true)] loop {} } } }
        ));
        assert!(!holds_spec(quote! { #[inline] let x = [spec]; }));
        assert!(!holds_spec(quote! { /// spec
        let spec = 1; }));
    }
}
