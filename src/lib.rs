use proc_macro2::TokenStream as TokenStream2;
use quote::{ToTokens, quote};
use syn::{
    Block, Expr, Pat, Token,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

struct ForStreams {
    arms: Vec<Arm>,
}

impl Parse for ForStreams {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Self {
            arms: parse_zero_or_more(input),
        })
    }
}

impl ToTokens for ForStreams {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let ForStreams { arms } = self;
        tokens.extend(quote! {
            {
                async fn async_fn_mut_for_each<S, F>(stream: S, mut f: F)
                where
                    S: ::futures::prelude::Stream,
                    F: ::std::ops::AsyncFnMut(S::Item),
                {
                    let mut stream = ::std::pin::pin!(stream);
                    while let Some(item) = ::futures::stream::StreamExt::next(&mut stream).await {
                        f(item).await;
                    }
                }
                ::futures::join! {
                    #(#arms),*
                };
            }
        });
    }
}

struct Arm {
    pattern: Pat,
    stream: Expr,
    body: Block,
    move_: bool,
}

impl Parse for Arm {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let pattern = Pat::parse_single(input)?;
        _ = input.parse::<Token![in]>()?;
        let stream = input.parse()?;
        _ = input.parse::<Token![=>]>()?;
        let move_ = input.parse::<Token![move]>().is_ok();
        let body = input.parse()?;
        Ok(Self {
            pattern,
            stream,
            body,
            move_,
        })
    }
}

impl ToTokens for Arm {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let Arm {
            pattern,
            stream,
            body,
            move_,
        } = self;
        if *move_ {
            tokens.extend(quote! {
                async_fn_mut_for_each(#stream, async move |#pattern| {
                    #body
                })
            });
        } else {
            tokens.extend(quote! {
                async_fn_mut_for_each(#stream, async |#pattern| {
                    #body
                })
            });
        }
    }
}

fn parse_zero_or_more<T: Parse>(input: ParseStream) -> Vec<T> {
    let mut result = Vec::new();
    while let Ok(item) = input.parse() {
        result.push(item);
    }
    result
}

#[proc_macro]
pub fn for_streams(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let c = parse_macro_input!(input as ForStreams);
    quote! { #c }.into()
}
