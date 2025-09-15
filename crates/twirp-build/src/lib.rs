#![doc = include_str!("../README.md")]

use quote::format_ident;
use syn::parse_quote;

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

struct Service {
    /// The name of the server trait, as parsed into a Rust identifier.
    rpc_trait_name: syn::Ident,

    /// The fully qualified protobuf name of this Service.
    fqn: String,

    /// The methods that make up this service.
    methods: Vec<Method>,
}

struct Method {
    /// The name of the method, as parsed into a Rust identifier.
    name: syn::Ident,

    /// The name of the method as it appears in the protobuf definition.
    proto_name: String,

    /// The input type of this method.
    input_type: syn::Type,

    /// The output type of this method.
    output_type: syn::Type,
}

impl Service {
    fn from_prost(s: prost_build::Service) -> Self {
        let fqn = format!("{}.{}", s.package, s.proto_name);
        let rpc_trait_name = format_ident!("{}", &s.name);
        let methods = s
            .methods
            .into_iter()
            .map(|m| Method::from_prost(&s.package, &s.proto_name, m))
            .collect();

        Self {
            rpc_trait_name,
            fqn,
            methods,
        }
    }
}

impl Method {
    fn from_prost(pkg_name: &str, svc_name: &str, m: prost_build::Method) -> Self {
        let as_type = |s| -> syn::Type {
            let Ok(typ) = syn::parse_str::<syn::Type>(s) else {
                panic!(
                    "twirp-build failed generated invalid Rust while processing {pkg}.{svc}/{name}). this is a bug in twirp-build, please file a GitHub issue",
                    pkg = pkg_name,
                    svc = svc_name,
                    name = m.proto_name,
                );
            };
            typ
        };

        let input_type = as_type(&m.input_type);
        let output_type = as_type(&m.output_type);
        let name = format_ident!("{}", m.name);
        let message = m.proto_name;

        Self {
            name,
            proto_name: message,
            input_type,
            output_type,
        }
    }
}

pub struct ServiceGenerator;

impl prost_build::ServiceGenerator for ServiceGenerator {
    fn generate(&mut self, service: prost_build::Service, buf: &mut String) {
        let service = Service::from_prost(service);

        // generate the twirp server
        let service_fqn_path = format!("/{}", service.fqn);
        let mut trait_methods: Vec<syn::TraitItemFn> = Vec::with_capacity(service.methods.len());
        let mut proxy_methods: Vec<syn::ImplItemFn> = Vec::with_capacity(service.methods.len());
        for m in &service.methods {
            let name = &m.name;
            let input_type = &m.input_type;
            let output_type = &m.output_type;

            trait_methods.push(parse_quote! {
                async fn #name(&self, req: twirp::Request<#input_type>) -> twirp::Result<twirp::Response<#output_type>>;
            });

            proxy_methods.push(parse_quote! {
                async fn #name(&self, req: twirp::Request<#input_type>) -> twirp::Result<twirp::Response<#output_type>> {
                    T::#name(&*self, req).await
                }
            });
        }

        let rpc_trait_name = &service.rpc_trait_name;
        let server_trait: syn::ItemTrait = parse_quote! {
            #[twirp::async_trait::async_trait]
            pub trait #rpc_trait_name: Send + Sync {
                #(#trait_methods)*
            }
        };
        let server_trait_impl: syn::ItemImpl = parse_quote! {
            #[twirp::async_trait::async_trait]
            impl<T> #rpc_trait_name for std::sync::Arc<T>
            where
                T: #rpc_trait_name + Sync + Send
            {
                #(#proxy_methods)*
            }
        };

        // generate the router
        let mut expr: syn::Expr = parse_quote! {
            twirp::details::TwirpRouterBuilder::new(#service_fqn_path, api)
        };
        for m in &service.methods {
            let name = &m.name;
            let input_type = &m.input_type;
            let path = format!("/{}", m.proto_name);

            expr = parse_quote! {
                #expr.route(#path, |api: T, req: twirp::Request<#input_type>| async move {
                    api.#name(req).await
                })
            };
        }
        let router: syn::ItemFn = parse_quote! {
            pub fn router<T>(api: T) -> twirp::Router
                where
                    T: #rpc_trait_name + Clone + Send + Sync + 'static
                {
                    #expr.build()
                }
        };

        //
        // generate the twirp client
        //
        let mut client_methods: Vec<syn::ImplItemFn> = Vec::with_capacity(service.methods.len());
        for m in &service.methods {
            let name = &m.name;
            let input_type = &m.input_type;
            let output_type = &m.output_type;
            let request_path = format!("{}/{}", service.fqn, m.proto_name);

            client_methods.push(parse_quote! {
                async fn #name(&self, req: twirp::Request<#input_type>) -> twirp::Result<twirp::Response<#output_type>> {
                    self.request(#request_path, req).await
                }
            })
        }
        let client_trait: syn::ItemImpl = parse_quote! {
            #[twirp::async_trait::async_trait]
            impl #rpc_trait_name for twirp::client::Client {
                #(#client_methods)*
            }
        };

        //
        // generate the client mock helpers
        //
        // TODO: Gate this code on a feature flag e.g. `std::env::var("CARGO_CFG_FEATURE_<FEATURE>").is_ok()`
        //
        let service_fqn = &service.fqn;
        let handler_name = format_ident!("{rpc_trait_name}Handler");
        let handler_struct: syn::ItemStruct = parse_quote! {
            pub struct #handler_name {
                inner: std::sync::Arc<dyn #rpc_trait_name>,
            }
        };
        let mut method_matches: Vec<syn::Arm> = Vec::with_capacity(service.methods.len());
        for m in &service.methods {
            let name = &m.name;
            let method = &m.proto_name;
            method_matches.push(parse_quote! {
                #method => {
                    twirp::details::encode_response(self.inner.#name(twirp::details::decode_request(req).await?).await?)
                }
            });
        }
        let handler_impl: syn::ItemImpl = parse_quote! {
            impl #handler_name {
                #[allow(clippy::new_ret_no_self)]
                pub fn new<M: #rpc_trait_name + 'static>(inner: M) -> Self {
                    Self { inner: std::sync::Arc::new(inner) }
                }
            }

        };
        let handler_direct_impl: syn::ItemImpl = parse_quote! {
            #[twirp::async_trait::async_trait]
            impl twirp::client::DirectHandler for #handler_name {
                fn service(&self) -> &str {
                    #service_fqn
                }
                async fn handle(&self, method: &str, req: twirp::reqwest::Request) -> twirp::Result<twirp::reqwest::Response> {
                    match method {
                        #(#method_matches)*
                        _ => Err(twirp::bad_route(format!("unknown rpc `{method}` for service `{}`, url: {:?}", #service_fqn, req.url()))),
                    }
                }
            }
        };
        let direct_api_handler: syn::ItemMod = parse_quote! {
            #[allow(dead_code)]
            pub mod handler {
                use super::*;

                #handler_struct
                #handler_impl
                #handler_direct_impl
            }
        };

        // generate the service and client as a single file. run it through
        // prettyplease before outputting it.
        let ast: syn::File = parse_quote! {
            pub use twirp;

            #server_trait
            #server_trait_impl

            #router

            #client_trait

            #direct_api_handler
        };

        let code = prettyplease::unparse(&ast);
        buf.push_str(&code);
    }
}
