use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{ToTokens, format_ident, quote};
use syn::{
    Block, Expr, Pat, Token,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

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
        let cancel_sender_ident = format_ident!("cancel_sender", span = Span::mixed_site());
        let mut arm_tokens = Vec::new();
        for arm in arms {
            let Arm {
                pattern,
                stream,
                body,
                is_move,
            } = arm;
            let move_token = if *is_move {
                quote! { move }
            } else {
                quote! {}
            };
            let returned_early_ident = format_ident!("returned_early", span = Span::mixed_site());
            let returned_early_ref_ident =
                format_ident!("returned_early_ref", span = Span::mixed_site());
            let stream_ident = format_ident!("stream", span = Span::mixed_site());
            arm_tokens.push(quote! {
                {
                    let #cancel_sender_ident = ::futures::channel::mpsc::Sender::clone(&#cancel_sender_ident);
                    async {
                        let mut #cancel_sender_ident = #cancel_sender_ident;
                        let mut #returned_early_ident = true;
                        // For the `move` case, we need to explicitly take a reference to
                        // `returned_early`, so that we don't copy it.
                        let #returned_early_ref_ident = &mut #returned_early_ident;
                        let _: () = async #move_token {
                            let mut #stream_ident = ::std::pin::pin!(#stream);
                            while let Some(#pattern) = ::futures::stream::StreamExt::next(&mut #stream_ident).await {
                                // NOTE: The #body may `continue`, `break`, or `return`.
                                #body
                            }
                            *#returned_early_ref_ident = false;
                        }.await;
                        if #returned_early_ident {
                            _ = ::futures::sink::SinkExt::send(&mut #cancel_sender_ident, ()).await;
                        }
                    }
                }
            });
        }
        let cancel_receiver_ident = format_ident!("cancel_receiver", span = Span::mixed_site());
        let joined_arms_ident = format_ident!("joined_arms", span = Span::mixed_site());
        tokens.extend(quote! {
            {
                // TODO: Support `return` without heap allocation somehow.
                let (#cancel_sender_ident, mut #cancel_receiver_ident) = ::futures::channel::mpsc::channel::<()>(0);
                let #joined_arms_ident = async {
                    ::futures::join! {
                        #(#arm_tokens),*
                    }
                };
                ::for_streams::_race(
                    #joined_arms_ident,
                    ::futures::stream::StreamExt::next(&mut #cancel_receiver_ident),
                ).await;
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
