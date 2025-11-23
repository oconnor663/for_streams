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
    is_move: bool,
}

impl Parse for Arm {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let pattern = Pat::parse_single(input)?;
        _ = input.parse::<Token![in]>()?;
        let stream = input.parse()?;
        _ = input.parse::<Token![=>]>()?;
        let is_move = input.parse::<Token![move]>().is_ok();
        let body = input.parse()?;
        Ok(Self {
            pattern,
            stream,
            body,
            is_move,
        })
    }
}

impl ToTokens for Arm {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let Arm {
            pattern,
            stream,
            body,
            is_move,
        } = self;
        let move_token = if *is_move {
            quote! { move }
        } else {
            quote! {}
        };
        tokens.extend(quote! {
            async #move_token {
                let mut stream = ::std::pin::pin!(#stream);
                while let Some(#pattern) = ::futures::stream::StreamExt::next(&mut stream).await {
                    let mut looped_once = false;
                    loop {
                        if looped_once {
                            break; // `continue` in the body
                        }
                        looped_once = true;
                        #body
                    }
                }
            }
        });
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
