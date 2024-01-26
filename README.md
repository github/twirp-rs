# twirp-rs

[Twirp is an RPC protocol](https://twitchtv.github.io/twirp/docs/spec_v7.html) based on HTTP and Protocol Buffers (proto). The protocol uses HTTP URLs to specify the RPC endpoints, and sends/receives proto messages as HTTP request/response bodies. Services are defined in a [.proto file](https://developers.google.com/protocol-buffers/docs/proto3), allowing easy implementation of RPC services with auto-generated clients and servers in different languages.

The [canonical implementation](https://github.com/twitchtv/twirp) is in Go, this is a Rust implementation of the protocol. Rust protocol buffer support is provided by the [`prost`](https://github.com/tokio-rs/prost) ecosystem.

Unlike [`prost-twirp`](https://github.com/sourcefrog/prost-twirp), the generated traits for serving and accessing RPCs are implemented atop `async` functions. Because traits containing `async` functions [are not directly supported](https://smallcultfollowing.com/babysteps/blog/2019/10/26/async-fn-in-traits-are-hard/) in Rust versions prior to 1.75, this crate uses the [`async_trait`](https://github.com/dtolnay/async-trait) macro to encapsulate the scaffolding required to make them work.

Versions of this package beginning with `0.1` are compatible with `hyper` 0.14. Versions beginning with `0.2` require `hyper` 1.1.

## Usage

See the [example](./example) for a complete example project.

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
twirp-build = "0.1"
prost-build = "0.12"
```

Add a `build.rs` file to your project to compile the protos and generate Rust code:

```rust
fn main() {
    let proto_source_files = ["./service.proto"];
    
    // Tell Cargo to rerun this build script if any of the proto files change
    for entry in &proto_source_files {
        println!("cargo:rerun-if-changed={}", entry);
    }

    prost_build::Config::new()
        .service_generator(twirp_build::service_generator())
        .compile_protos(&proto_source_files, &["./"])
        .expect("error compiling protos");
}
```

This generates code that you can find in `target/build/your-project-*/out/example.service.rs`. In order to use this code, you'll need to implement the trait for the proto defined service and wire up the service handlers to a hyper web server. See [the example `main.rs`]( example/src/main.rs) for details.

Include the generated code, create a router, register your service, and then serve those routes in the hyper server:

```rust
mod haberdash {
    include!(concat!(env!("OUT_DIR"), "/service.haberdash.v1.rs"));
}
use haberdash

#[tokio::main]
pub async fn main() {
    let mut router = Router::default();
    let server = Arc::new(HaberdasherAPIServer {});
    haberdash::add_service(&mut router, server.clone());
    let router = Arc::new(router);
    let service = make_service_fn(move |_| {
        let router = router.clone();
        async { Ok::<_, GenericError>(service_fn(move |req| twirp::serve(router.clone(), req))) }
    });

    let addr = ([127, 0, 0, 1], 3000).into();
    let server = Server::bind(&addr).serve(service);
    server.await.expect("server error")
}

// Define the server and implement the trait.
struct HaberdasherAPIServer;

#[async_trait]
impl haberdash::HaberdasherAPI for HaberdasherAPIServer {
    async fn make_hat(&self, req: MakeHatRequest) -> Result<MakeHatResponse, TwirpErrorResponse> {
        todo!()
    }
}
```

## Usage (client side)

On the client side, you also get a generated twirp client (based on the rpc endpoints in your proto). Include the generated code, create a client, and start making rpc calls:

``` rust
mod haberdash {
    include!(concat!(env!("OUT_DIR"), "/service.haberdash.v1.rs"));
}

use haberdash::{HaberdasherAPIClient, MakeHatRequest, MakeHatResponse};

#[tokio::main]
pub async fn main() {
    let client = Client::from_base_url(Url::parse("http://localhost:3000/twirp/")?)?;
    let resp = client.make_hat(MakeHatRequest { inches: 1 }).await;
    eprintln!("{:?}", resp);
}
```
