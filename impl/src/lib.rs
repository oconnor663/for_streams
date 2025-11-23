use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{ToTokens, format_ident, quote};
use syn::{
    Block, Expr, Pat, Token,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

struct Arm {
    pattern: Pat,
    stream_expr: Expr,
    body: Block,
    is_move: bool,
}

impl Parse for Arm {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let pattern = Pat::parse_single(input)?;
        _ = input.parse::<Token![in]>()?;
        let stream_expr = input.parse()?;
        _ = input.parse::<Token![=>]>()?;
        let is_move = input.parse::<Token![move]>().is_ok();
        let body = input.parse()?;
        Ok(Self {
            pattern,
            stream_expr,
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
        let mut arms = Vec::new();
        while !input.is_empty() {
            arms.push(input.parse()?);
        }
        Ok(Self { arms })
    }
}

impl ToTokens for ForStreams {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let ForStreams { arms } = self;
        let cancel_sender = format_ident!("cancel_sender", span = Span::mixed_site());
        let mut arm_tokens = Vec::new();
        for arm in arms {
            let Arm {
                pattern,
                stream_expr,
                body,
                is_move,
            } = arm;
            let move_token = if *is_move {
                quote! { move }
            } else {
                quote! {}
            };
            let returned_early = format_ident!("returned_early", span = Span::mixed_site());
            let returned_early_ref = format_ident!("returned_early_ref", span = Span::mixed_site());
            let stream = format_ident!("stream", span = Span::mixed_site());
            arm_tokens.push(quote! {
                {
                    let #cancel_sender = ::futures::channel::mpsc::Sender::clone(&#cancel_sender);
                    async {
                        let mut #cancel_sender = #cancel_sender;
                        let mut #returned_early = true;
                        // For the `move` case, we need to explicitly take a reference to
                        // `returned_early`, so that we don't copy it.
                        let #returned_early_ref = &mut #returned_early;
                        let _: () = async #move_token {
                            let mut #stream = ::std::pin::pin!(#stream_expr);
                            while let Some(#pattern) = ::futures::stream::StreamExt::next(&mut #stream).await {
                                // NOTE: The #body may `continue`, `break`, or `return`.
                                #body
                            }
                            *#returned_early_ref = false;
                        }.await;
                        if #returned_early {
                            _ = ::futures::sink::SinkExt::send(&mut #cancel_sender, ()).await;
                        }
                    }
                }
            });
        }
        let cancel_receiver = format_ident!("cancel_receiver", span = Span::mixed_site());
        tokens.extend(quote! {
            {
                // TODO: Support `return` without heap allocation somehow.
                let (#cancel_sender, mut #cancel_receiver) = ::futures::channel::mpsc::channel::<()>(0);
                ::for_streams::_race(
                    async {
                        ::futures::join! {
                            #(#arm_tokens),*
                        }
                    },
                    ::futures::stream::StreamExt::next(&mut #cancel_receiver),
                ).await;
            }
        });
    }
}

#[proc_macro]
pub fn for_streams(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let c = parse_macro_input!(input as ForStreams);
    quote! { #c }.into()
}
