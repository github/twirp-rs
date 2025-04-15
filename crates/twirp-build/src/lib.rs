use proc_macro2::TokenStream;
use quote::{format_ident, quote, ToTokens};

/// Generates twirp services for protobuf rpc service definitions.
///
/// In your `build.rs`, using `prost_build`, you can wire in the twirp
/// `ServiceGenerator` to produce a Rust server for your proto services.
///
/// Add a call to `.service_generator(twirp_build::service_generator())` in
/// main() of `build.rs`.
pub fn service_generator() -> Box<ServiceGenerator> {
    Box::new(ServiceGenerator {})
}

pub struct ServiceGenerator;

fn as_path(s: &str) -> TokenStream {
    syn::parse_str::<syn::Path>(s)
        .expect("twirp-build generated invalid Rust. this is a bug in twirp-build, please file an issue")
        .to_token_stream()
}

impl prost_build::ServiceGenerator for ServiceGenerator {
    fn generate(&mut self, service: prost_build::Service, buf: &mut String) {
        let service_name = format_ident!("{}", &service.name);
        let service_fqn = format!("{}.{}", service.package, service.proto_name);

        // generate the twirp server
        let mut trait_methods = Vec::with_capacity(service.methods.len());
        let mut proxy_methods = Vec::with_capacity(service.methods.len());
        for m in &service.methods {
            let name = format_ident!("{}", &m.name);
            let input_type = as_path(&m.input_type);
            let output_type = as_path(&m.output_type);

            trait_methods.push(quote! {
                async fn #name(&self, ctx: twirp::Context, req: #input_type) -> Result<#output_type, Self::Error>;
            });

            proxy_methods.push(quote! {
                async fn #name(&self, ctx: twirp::Context, req: #input_type) -> Result<#output_type, Self::Error> {
                    T::#name(&*self, ctx, req).await
                }
            });
        }

        let server_trait = quote! {
            #[twirp::async_trait::async_trait]
            pub trait #service_name {
                type Error;

                #(#trait_methods)*
            }

            #[twirp::async_trait::async_trait]
            impl<T> #service_name for std::sync::Arc<T>
            where
                T: #service_name + Sync + Send
            {
                type Error = T::Error;

                #(#proxy_methods)*
            }
        };

        // generate the router
        let mut route_calls = Vec::with_capacity(service.methods.len());
        for m in &service.methods {
            let name = format_ident!("{}", &m.name);
            let uri = format!("/{}", &m.proto_name);
            let req_type = as_path(&m.input_type);
            route_calls.push(quote! {
                .route(#uri, |api: T, ctx: twirp::Context, req: #req_type| async move {
                    api.#name(ctx, req).await
                })
            });
        }
        let router = quote! {
            pub fn router<T>(api: T) -> twirp::Router
                where
                    T: #service_name + Clone + Send + Sync + 'static,
                    <T as #service_name>::Error: twirp::IntoTwirpResponse
                {
                    twirp::details::TwirpRouterBuilder::new(api)
                        #(#route_calls).*
                        .build()
                }
        };

        //
        // generate the twirp client
        //
        let client_name = format_ident!("{}Client", service_name);

        let mut client_trait_methods = Vec::with_capacity(service.methods.len());
        let mut client_methods = Vec::with_capacity(service.methods.len());
        for m in &service.methods {
            let name = format_ident!("{}", &m.name);
            let input_type = as_path(&m.input_type);
            let output_type = as_path(&m.output_type);

            client_trait_methods.push(quote! {
                async fn #name(&self, req: #input_type) -> Result<#output_type, twirp::ClientError>;
            });

            let url = format!("{}/{}", service_fqn, m.proto_name);
            client_methods.push(quote! {
                async fn #name(&self, req: #input_type) -> Result<#output_type, twirp::ClientError> {
                    self.request(#url, req).await
                }
            })
        }
        let client_trait = quote! {
            #[twirp::async_trait::async_trait]
            pub trait #client_name: Send + Sync {
                #(#client_trait_methods)*
            }

            #[twirp::async_trait::async_trait]
            impl #client_name for twirp::client::Client {
                #(#client_methods)*
            }
        };

        // generate the service and client as a single file. run it through
        // prettyplease before outputting it.
        let service_fqn_path = format!("/{}", service_fqn);
        let generated = quote! {
            pub use twirp;

            pub const SERVICE_FQN: &str = #service_fqn_path;

            #server_trait

            #router

            #client_trait
        };

        let ast: syn::File = syn::parse2(generated).expect("generated an invalid token stream");
        let code = prettyplease::unparse(&ast);
        buf.push_str(&code);
    }
}
