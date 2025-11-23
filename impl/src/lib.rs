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
                // TODO: Support `break` without heap allocation somehow.
                let (break_sender, mut break_receiver) = ::futures::channel::mpsc::channel::<()>(0);
                let joined_arms = async {
                    ::futures::join! {
                        #(#arms),*
                    }
                };
                ::for_streams::_race(
                    joined_arms,
                    ::futures::stream::StreamExt::next(&mut break_receiver),
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
                let break_sender = ::futures::channel::mpsc::Sender::clone(&break_sender);
                async #move_token {
                    let mut break_sender = break_sender;
                    let mut stream = ::std::pin::pin!(#stream);
                    let mut end_of_stream = false;
                    loop {
                        match ::futures::stream::StreamExt::next(&mut stream).await {
                            Some(#pattern) => {
                                // NOTE: The body may `continue`, `break`, or `return`.
                                #body
                            }
                            None => {
                                end_of_stream = true;
                                break;
                            }
                        }
                    }
                    if !end_of_stream {
                        // `break` in the #body
                        _ = ::futures::sink::SinkExt::send(&mut break_sender, ()).await;
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
