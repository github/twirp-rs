# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.9.1](https://github.com/github/twirp-rs/compare/twirp-v0.9.0...twirp-v0.9.1) - 2025-08-14

### Fixed

- Preserve HTTP version in twirp client response ([#235](https://github.com/github/twirp-rs/pull/235))

### Other

- Bump tokio from 1.46.1 to 1.47.1 ([#231](https://github.com/github/twirp-rs/pull/231))

## [0.9.0](https://github.com/github/twirp-rs/compare/twirp-build-v0.8.0...twirp-build-v0.9.0) - 2025-07-31

### Breaking

- Remove `SERVICE_FQN` to avoid upgrade confusion ([#222](https://github.com/github/twirp-rs/pull/222))

#### Breaking: Allow custom headers and extensions for twirp clients and servers; unify traits; unify error type ([#212](https://github.com/github/twirp-rs/pull/212))

- No more `Context`. The same capabilities now exist via http request and response [Extensions](https://docs.rs/http/latest/http/struct.Extensions.html) and [Headers](https://docs.rs/http/latest/http/header/struct.HeaderMap.html).
- Clients and servers now share a single trait (the rpc interface).
- It is possible to set custom headers on requests (client side) and it's possible for server handlers to read request headers and set custom response headers.
- The same ☝🏻 is true for extensions to allow interactivity with middleware.
- All the above is accomplished by using `http::request::Request<In>` and `http::response::Response<Out>` where `In` and `Out` are the individual rpc message types.
- We have unifyied and simplified the error types. There is now just `TwirpErrorResponse` which models the [twirp error response spec](https://twitchtv.github.io/twirp/docs/spec_v7.html#error-codes).


#### Breaking: Generate service fqn ([#221](https://github.com/github/twirp-rs/pull/221))

Applications will need to remove any manual service nesting they are doing today.

In 0.8.0, server consumers of this library have to know how to properly construct the fully qualified service path by using `nest` on an `axum` `Router` like so:

```rust
let twirp_routes = Router::new()
        .nest(haberdash::SERVICE_FQN, haberdash::router(api_impl));
```

This is unnecessary in 0.9.0 (the generated `router` function for each service does that for you). Instead, you would write:

``` rust
let twirp_routes = haberdash::router(api_impl);
```

It is still canonical (but not required) to then nest with a `/twirp` prefix (the examples show this).

### Other

- Allow mocking out requests ([#220](https://github.com/github/twirp-rs/pull/220))
- Swap twirp and twirp-build readmes. Replace the repo readme with a symlink to twirp's readme. ([#215](https://github.com/github/twirp-rs/pull/215))
- Update the content of the readme ([#216](https://github.com/github/twirp-rs/pull/216))
- Include the readme in rustdoc ([#225](https://github.com/github/twirp-rs/pull/225))
