use quote::{format_ident, quote};

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
        let mut trait_methods = Vec::with_capacity(service.methods.len());
        let mut proxy_methods = Vec::with_capacity(service.methods.len());
        for m in &service.methods {
            let name = &m.name;
            let input_type = &m.input_type;
            let output_type = &m.output_type;

            trait_methods.push(quote! {
                async fn #name(&self, req: twirp::Request<#input_type>) -> twirp::Result<twirp::Response<#output_type>>;
            });

            proxy_methods.push(quote! {
                async fn #name(&self, req: twirp::Request<#input_type>) -> twirp::Result<twirp::Response<#output_type>> {
                    T::#name(&*self, req).await
                }
            });
        }

        let rpc_trait_name = &service.rpc_trait_name;
        let server_trait = quote! {
            #[twirp::async_trait::async_trait]
            pub trait #rpc_trait_name: Send + Sync {
                #(#trait_methods)*
            }

            #[twirp::async_trait::async_trait]
            impl<T> #rpc_trait_name for std::sync::Arc<T>
            where
                T: #rpc_trait_name + Sync + Send
            {
                #(#proxy_methods)*
            }
        };

        // generate the router
        let mut route_calls = Vec::with_capacity(service.methods.len());
        for m in &service.methods {
            let name = &m.name;
            let input_type = &m.input_type;
            let path = format!("/{uri}", uri = m.proto_name);

            route_calls.push(quote! {
                .route(#path, |api: T, req: twirp::Request<#input_type>| async move {
                    api.#name(req).await
                })
            });
        }
        let router = quote! {
            pub fn router<T>(api: T) -> twirp::Router
                where
                    T: #rpc_trait_name + Clone + Send + Sync + 'static
                {
                    twirp::details::TwirpRouterBuilder::new(api)
                        #(#route_calls)*
                        .build()
                }
        };

        //
        // generate the twirp client
        //
        let mut client_methods = Vec::with_capacity(service.methods.len());
        for m in &service.methods {
            let name = &m.name;
            let input_type = &m.input_type;
            let output_type = &m.output_type;
            let request_path = format!("{}/{}", service.fqn, m.proto_name);

            client_methods.push(quote! {
                async fn #name(&self, req: twirp::Request<#input_type>) -> twirp::Result<twirp::Response<#output_type>> {
                    self.request(#request_path, req).await
                }
            })
        }
        let client_trait = quote! {
            #[twirp::async_trait::async_trait]
            impl #rpc_trait_name for twirp::client::Client {
                #(#client_methods)*
            }
        };

        // generate the service and client as a single file. run it through
        // prettyplease before outputting it.
        let service_fqn_path = format!("/{}", service.fqn);
        let generated = quote! {
            pub use twirp;

            pub const SERVICE_FQN: &str = #service_fqn_path;

            #server_trait

            #router

            #client_trait
        };

        let ast: syn::File = syn::parse2(generated)
            .expect("twirp-build generated invalid Rust. this is a bug in twirp-build, please file an issue");
        let code = prettyplease::unparse(&ast);
        buf.push_str(&code);
    }
}
