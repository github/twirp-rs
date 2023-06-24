# twirp-rs

[Twirp is an RPC protocol](https://twitchtv.github.io/twirp/docs/spec_v7.html) based on HTTP and Protocol Buffers (proto). The protocol uses HTTP URLs to specify the RPC endpoints, and sends/receives proto messages as HTTP request/response bodies. Services are defined in a [.proto file](https://developers.google.com/protocol-buffers/docs/proto3), allowing easy implementation of RPC services with auto-generated clients and servers in different languages.

The [canonical implementation](https://github.com/twitchtv/twirp) is in Golang, this is a Rust implementation of the protocol. Currently, this crate only supports server generation, client generation is a future TODO.

## Usage

See the [example](./example) for a complete example project.

Define services and messages in a `.proto` file:

```proto
// example.service.proto
service HaberdasherAPI {
   rpc MakeHat(MakeHatRequest) returns (MakeHatResponse);
}
message MakeHatRequest { }
message MakeHatResponse { }
```

Add the `twirp-build` crate as a dependency in your `Cargo.toml` (you'll need `prost-build` too):

```toml
# Cargo.toml
[build-dependencies]
twirp-build = "0.1"
prost-build = "0.11"
```

Add a `build.rs` file to your project to compile the protos and generate Rust code:

```rust
fn main() {
    prost_build::Config::new()
        .service_generator(twirp_build::service_generator())
        .compile_protos(&["./service.proto"], &["./"])
        .expect("error compiling protos");
}
```

This generates code that you can find in `target/build/your-project-*/out/example.service.rs`. In order to use this code, you'll need to implement the trait for the proto defined service and wire up the service handlers to a hyper web server. See [the example `main.rs`]( example/src/main.rs) for details.

Essentially, you need to include the generate code, create a router, register your service, and then serve those routes in the hyper server:

```rust
pub mod service {
    pub mod haberdash {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/service.haberdash.v1.rs"));
        }
    }
}
use service::haberdash::v1:: as haberdash;

#[tokio::main]
pub async fn main() {
    let mut router = Router::default();
    let example = Arc::new(HaberdasherAPIServer {});
    haberdash::add_service(&mut router, example.clone());
    let router = Arc::new(router);
    let service = make_service_fn(move |_| {
        let router = router.clone();
        async { Ok::<_, GenericError>(service_fn(move |req| twirp::serve(router.clone(), req))) }
    });
}
```
