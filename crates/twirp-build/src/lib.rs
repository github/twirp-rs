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
    server_name: syn::Ident,

    /// The name of the client trait, as parsed into a Rust identifier.
    client_name: syn::Ident,

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
        let server_name = format_ident!("{}", &s.name);
        let client_name = format_ident!("{}Client", &s.name);
        let methods = s
            .methods
            .into_iter()
            .map(|m| Method::from_prost(&s.package, &s.proto_name, m))
            .collect();

        Self {
            server_name,
            client_name,
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
                async fn #name(&self, ctx: twirp::Context, req: #input_type) -> Result<#output_type, Self::Error>;
            });

            proxy_methods.push(quote! {
                async fn #name(&self, ctx: twirp::Context, req: #input_type) -> Result<#output_type, Self::Error> {
                    T::#name(&*self, ctx, req).await
                }
            });
        }

        let server_name = &service.server_name;
        let server_trait = quote! {
            #[twirp::async_trait::async_trait]
            pub trait #server_name {
                type Error;

                #(#trait_methods)*
            }

            #[twirp::async_trait::async_trait]
            impl<T> #server_name for std::sync::Arc<T>
            where
                T: #server_name + Sync + Send
            {
                type Error = T::Error;

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
                .route(#path, |api: T, ctx: twirp::Context, req: #input_type| async move {
                    api.#name(ctx, req).await
                })
            });
        }
        let router = quote! {
            pub fn router<T>(api: T) -> twirp::Router
                where
                    T: #server_name + Clone + Send + Sync + 'static,
                    <T as #server_name>::Error: twirp::IntoTwirpResponse
                {
                    twirp::details::TwirpRouterBuilder::new(api)
                        #(#route_calls)*
                        .build()
                }
        };

        //
        // generate the twirp client
        //

        let client_name = service.client_name;
        let mut client_trait_methods = Vec::with_capacity(service.methods.len());
        let mut client_methods = Vec::with_capacity(service.methods.len());
        for m in &service.methods {
            let name = &m.name;
            let build_name = format_ident!("build_{}", name);
            let input_type = &m.input_type;
            let output_type = &m.output_type;
            let request_path = format!("{}/{}", service.fqn, m.proto_name);

            client_trait_methods.push(quote! {
                async fn #name(&self, req: #input_type) -> Result<#output_type, twirp::ClientError>;
            });
            client_trait_methods.push(quote! {
                fn #build_name(&self, req: #input_type) -> Result<twirp::RequestBuilder<#input_type, #output_type>, twirp::ClientError>;
            });

            client_methods.push(quote! {
                fn #build_name(&self, req: #input_type) -> Result<twirp::RequestBuilder<#input_type, #output_type>, twirp::ClientError> {
                    self.build_request(#request_path, req)
                }
            });
            client_methods.push(quote! {
                async fn #name(&self, req: #input_type) -> Result<#output_type, twirp::ClientError> {
                    let builder = self.#build_name(req)?;
                    self.request(builder).await
                }
            });
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
