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
                // TODO: Support `return` without heap allocation somehow.
                let (cancel_sender, mut cancel_receiver) = ::futures::channel::mpsc::channel::<()>(0);
                let joined_arms = async {
                    ::futures::join! {
                        #(#arms),*
                    }
                };
                ::for_streams::_race(
                    joined_arms,
                    ::futures::stream::StreamExt::next(&mut cancel_receiver),
                ).await;
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
            {
                // TODO: catch `return` somehow
                let cancel_sender = ::futures::channel::mpsc::Sender::clone(&cancel_sender);
                async {
                    let mut cancel_sender = cancel_sender;
                    let mut returned_early = true;
                    // For the `move` case, we need to explicitly take a reference to
                    // `returned_early`, so that we don't copy it.
                    let returned_early_ref = &mut returned_early;
                    let _: () = async #move_token {
                        let mut stream = ::std::pin::pin!(#stream);
                        while let Some(#pattern) = ::futures::stream::StreamExt::next(&mut stream).await {
                            // NOTE: The #body may `continue`, `break`, or `return`.
                            #body
                        }
                        *returned_early_ref = false;
                    }.await;
                    if returned_early {
                        _ = ::futures::sink::SinkExt::send(&mut cancel_sender, ()).await;
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
