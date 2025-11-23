use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{ToTokens, format_ident, quote};
use syn::{
    Block, Expr, Ident, Pat, Token,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

mod kw {
    syn::custom_keyword!(background);
}

struct Arm {
    pattern: Pat,
    stream_expr: Expr,
    body: Block,
    is_background: bool,
    is_move: bool,
}

impl Parse for Arm {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let pattern = Pat::parse_single(input)?;
        _ = input.parse::<Token![in]>()?;
        // Check whether we can parse a stream expression after `background`. If not, `background`
        // itself could be the stream expression (i.e. a local variable name).
        let fork = input.fork();
        let is_background = fork.parse::<kw::background>().is_ok() && fork.parse::<Expr>().is_ok();
        if is_background {
            _ = input.parse::<kw::background>()?;
        }
        let stream_expr = input.parse()?;
        _ = input.parse::<Token![=>]>()?;
        let is_move = input.parse::<Token![move]>().is_ok();
        let body = input.parse()?;
        Ok(Self {
            pattern,
            stream_expr,
            body,
            is_background,
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
            let arm = input.parse::<Arm>()?;
            arms.push(arm);
        }
        Ok(Self { arms })
    }
}

impl ToTokens for ForStreams {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let mut initializers = TokenStream2::new();
        let cancel_sender = format_ident!("cancel_sender", span = Span::mixed_site());
        let arm_names: Vec<Ident> = (0..self.arms.len())
            .map(|i| format_ident!("arm_{}", i, span = Span::mixed_site()))
            .collect();
        for i in 0..self.arms.len() {
            let Arm {
                pattern,
                stream_expr,
                body,
                is_background: _,
                is_move,
            } = &self.arms[i];
            let move_token = if *is_move {
                quote! { move }
            } else {
                quote! {}
            };
            let returned_early = format_ident!("returned_early", span = Span::mixed_site());
            let returned_early_ref = format_ident!("returned_early_ref", span = Span::mixed_site());
            let stream = format_ident!("stream", span = Span::mixed_site());
            let name = &arm_names[i];
            initializers.extend(quote! {
                let mut #name = ::std::pin::pin!(::futures::future::FutureExt::fuse({
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
                }));
            });
        }

        let mut poll_calls = TokenStream2::new();
        let foreground_finished = format_ident!("foreground_finished", span = Span::mixed_site());
        let cx = format_ident!("cx", span = Span::mixed_site());
        for i in 0..self.arms.len() {
            let name = &arm_names[i];
            poll_calls.extend(quote! {
                // NOTE: These are fused, so we can poll them unconditionally.
                _ = ::std::future::Future::poll(::std::pin::Pin::as_mut(&mut #name), #cx);
            });
            if !self.arms[i].is_background {
                poll_calls.extend(quote! {
                    #foreground_finished &= ::futures::future::FusedFuture::is_terminated(&#name);
                });
            }
        }

        let cancel_receiver = format_ident!("cancel_receiver", span = Span::mixed_site());
        tokens.extend(quote! {
            {
                // TODO: Support `return` without heap allocation somehow.
                let (#cancel_sender, #cancel_receiver) = ::futures::channel::mpsc::channel::<()>(0);
                #initializers
                let mut #cancel_receiver = ::std::pin::pin!(#cancel_receiver);
                ::std::future::poll_fn(|#cx| {
                    let mut #foreground_finished = true;
                    if ::futures::stream::Stream::poll_next(::std::pin::Pin::as_mut(&mut #cancel_receiver), #cx).is_ready() {
                        return ::std::task::Poll::Ready(());
                    }
                    #poll_calls
                    if #foreground_finished {
                        ::std::task::Poll::Ready(())
                    } else {
                        ::std::task::Poll::Pending
                    }
                }).await;
            }
        });
    }
}

#[proc_macro]
pub fn for_streams(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let c = parse_macro_input!(input as ForStreams);
    quote! { #c }.into()
}
