# `twirp`

[Twirp is an RPC protocol](https://twitchtv.github.io/twirp/docs/spec_v7.html) based on HTTP and Protocol Buffers (proto). The protocol uses HTTP URLs to specify the RPC endpoints, and sends/receives proto messages as HTTP request/response bodies. Services are defined in a [.proto file](https://developers.google.com/protocol-buffers/docs/proto3), allowing easy implementation of RPC services with auto-generated clients and servers in different languages.

The [canonical implementation](https://github.com/twitchtv/twirp) is in Go, and this is a Rust implementation of the protocol. Rust protocol buffer support is provided by the [`prost`](https://github.com/tokio-rs/prost) ecosystem.

Unlike [`prost-twirp`](https://github.com/sourcefrog/prost-twirp), the generated traits for serving and accessing RPCs are implemented atop `async` functions. Because traits containing `async` functions [are not directly supported](https://smallcultfollowing.com/babysteps/blog/2019/10/26/async-fn-in-traits-are-hard/) in Rust versions prior to 1.75, this crate uses the [`async_trait`](https://github.com/dtolnay/async-trait) macro to encapsulate the scaffolding required to make them work.

## Usage

See the [example](https://github.com/github/twirp-rs/tree/main/example) for a complete example project.

Define services and messages in a `.proto` file:

```proto
// service.proto
package service.haberdash.v1;

service HaberdasherAPI {
   rpc MakeHat(MakeHatRequest) returns (MakeHatResponse);
}
message MakeHatRequest { }
message MakeHatResponse { }
```

Add the `twirp-build` crate as a build dependency in your `Cargo.toml` (you'll need `prost-build` too):

```toml
# Cargo.toml
[build-dependencies]
twirp-build = "0.7"
prost-build = "0.13"
```

Add a `build.rs` file to your project to compile the protos and generate Rust code:

<!--
`rust` gets syntax highlighting in GitHub UI and Rustdoc contexts
` ,ignore` (including the space) makes rust tests ignore the code without breaking syntax highlighting
-->
```rust ,ignore
fn main() {
    let proto_source_files = ["./service.proto"];

    // Tell Cargo to rerun this build script if any of the proto files change
    for entry in &proto_source_files {
        println!("cargo:rerun-if-changed={}", entry);
    }

    prost_build::Config::new()
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]") // enable support for JSON encoding
        .service_generator(twirp_build::service_generator())
        .compile_protos(&proto_source_files, &["./"])
        .expect("error compiling protos");
}
```

This generates code that you can find in `target/build/your-project-*/out/example.service.rs`. In order to use this code, you'll need to implement the trait for the proto defined service and wire up the service handlers to a hyper web server. See [the example](https://github.com/github/twirp-rs/tree/main/example) for details.

Include the generated code, create a router, register your service, and then serve those routes in the hyper server:

<!--
`rust` gets syntax highlighting in GitHub UI and Rustdoc contexts
` ,ignore` (including the space) makes rust tests ignore the code without breaking syntax highlighting
-->
```rust ,ignore
mod haberdash {
    include!(concat!(env!("OUT_DIR"), "/service.haberdash.v1.rs"));
}

use axum::Router;
use haberdash::{MakeHatRequest, MakeHatResponse};

#[tokio::main]
pub async fn main() {
    let api_impl = Arc::new(HaberdasherApiServer {});
    let app = Router::new()
        .nest("/twirp", haberdash::router(api_impl))
        .fallback(twirp::server::not_found_handler);

    let tcp_listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    if let Err(e) = axum::serve(tcp_listener, app).await {
        eprintln!("server error: {}", e);
    }
}

// Define the server and implement the trait.
struct HaberdasherApiServer;

#[async_trait]
impl haberdash::HaberdasherApi for HaberdasherApiServer {
    async fn make_hat(&self, req: twirp::Request<MakeHatRequest>) -> twirp::Result<twirp::Response<MakeHatResponse>> {
        todo!()
    }
}
```

This code creates an `axum::Router`, then hands it off to `axum::serve()` to handle networking. This use of `axum::serve` is optional. After building `app`, you can instead invoke it from any `hyper`-based server by importing `twirp::tower::Service` and doing `app.call(request).await`.

## Usage (client side)

On the client side, you also get a generated twirp client (based on the rpc endpoints in your proto). Include the generated code, create a client, and start making rpc calls:

<!--
`rust` gets syntax highlighting in GitHub UI and Rustdoc contexts
` ,ignore` (including the space) makes rust tests ignore the code without breaking syntax highlighting
-->
```rust ,ignore
mod haberdash {
    include!(concat!(env!("OUT_DIR"), "/service.haberdash.v1.rs"));
}

use haberdash::{HaberdasherApiClient, MakeHatRequest, MakeHatResponse};

#[tokio::main]
pub async fn main() {
    let client = Client::from_base_url(Url::parse("http://localhost:3000/twirp/")?)?;
    let resp = client.make_hat(MakeHatRequest { inches: 1 }).await;
    eprintln!("{:?}", resp);
}
```
## Minimum supported Rust version

The MSRV for this crate is the version defined in [`rust-toolchain.toml`](https://github.com/github/twirp-rs/blob/main/rust-toolchain.toml)

## Getting Help

You are welcome to open an [issue](https://github.com/github/twirp-rs/issues/new) with your question.

## Contributing

🎈 Thanks for your help improving the project! We are so happy to have you! We have a [contributing guide](https://github.com/github/twirp-rs/blob/main/CONTRIBUTING.md) to help you get involved in the project.

## License

This project is licensed under the [MIT license](https://github.com/github/twirp-rs/blob/main/LICENSE).
