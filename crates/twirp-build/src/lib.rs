use std::fmt::Write;

/// Generates twirp services for protobuf rpc service definitions.
///
/// In your `build.rs`, using `prost_build`, you can wire in the twirp
/// `ServiceGenerator` to produce a Rust server for your proto services.
///
/// Add a call to `.service_generator(twirp_build::service_generator())` in
/// main() of `build.rs`:
pub fn service_generator() -> Box<ServiceGenerator> {
    Box::new(ServiceGenerator {})
}

pub struct ServiceGenerator;

impl prost_build::ServiceGenerator for ServiceGenerator {
    fn generate(&mut self, service: prost_build::Service, buf: &mut String) {
        let service_name = service.name.replace("Api", "API");
        let service_fqn = format!("{}.{}", service.package, service_name);
        writeln!(buf).unwrap();
        writeln!(buf, "#[async_trait::async_trait]").unwrap();
        writeln!(buf, "pub trait {} {{", service_name).unwrap();
        for m in &service.methods {
            writeln!(
                buf,
                "    async fn {}(&self, req: {}) -> Result<{}, twirp::TwirpErrorResponse>;",
                m.name, m.input_type, m.output_type,
            )
            .unwrap();
        }
        writeln!(buf, "}}").unwrap();

        // add_service
        writeln!(
            buf,
            r#"pub fn add_service<T>(router: &mut twirp::Router, api: std::sync::Arc<T>)
where
    T: {} + Send + Sync + 'static,
{{"#,
            service_name
        )
        .unwrap();
        for m in &service.methods {
            writeln!(
                buf,
                r#"    {{
        #[allow(clippy::redundant_clone)]
        let api = api.clone();
        router.add_method(
            "{}/{}",
            move |req| {{
                let api = api.clone();
                async move {{ api.{}(req).await }}
            }},
        );
    }}"#,
                service_fqn, m.proto_name, m.name
            )
            .unwrap();
        }
        writeln!(buf, "}}").unwrap();

        //
        // generate the twirp client
        //
        writeln!(buf).unwrap();
        // top level trait
        writeln!(buf, "#[async_trait::async_trait]").unwrap();
        writeln!(buf, "pub trait {}Client {{", service_name).unwrap();
        for m in &service.methods {
            writeln!(
                buf,
                "    async fn {}(&self, req: {}) -> Result<{}, twirp::client::TwirpClientError>;",
                m.name, m.input_type, m.output_type,
            )
            .unwrap();
        }
        writeln!(buf, "}}").unwrap();

        // Ext trait
        writeln!(buf, "#[async_trait::async_trait]").unwrap();
        writeln!(buf, "pub trait {}ClientExt {{", service_name).unwrap();
        for m in &service.methods {
            writeln!(
                buf,
                "    fn {}_url(&self, base_url: &twirp::url::Url) -> Result<twirp::url::Url, twirp::client::TwirpClientError> {{",
                m.name,
            )
            .unwrap();
            writeln!(
                buf,
                r#"    let url = base_url.join("{}/{}")?;"#,
                service_fqn, m.proto_name,
            )
            .unwrap();
            writeln!(buf, "    Ok(url)").unwrap();
            writeln!(buf, "    }}").unwrap();

            writeln!(
                buf,
                "    async fn {}_with_url(&self, url: twirp::url::Url, req: {}) -> Result<{}, twirp::client::TwirpClientError>;",
                m.name, m.input_type, m.output_type,
            )
            .unwrap();
        }
        writeln!(buf, "}}").unwrap();

        // Implement the traits
        writeln!(buf, "#[async_trait::async_trait]").unwrap();
        writeln!(
            buf,
            "impl {}Client for twirp::client::TwirpClient {{",
            service_name
        )
        .unwrap();
        for m in &service.methods {
            writeln!(
                buf,
                "    async fn {}(&self, req: {}) -> Result<{}, twirp::client::TwirpClientError> {{",
                m.name, m.input_type, m.output_type,
            )
            .unwrap();
            writeln!(
                buf,
                "    self.{}_with_url(self.{}_url(&self.base_url)?, req).await",
                m.name, m.name
            )
            .unwrap();
            writeln!(buf, "    }}").unwrap();
        }
        writeln!(buf, "}}").unwrap();

        writeln!(buf, "#[async_trait::async_trait]").unwrap();
        writeln!(
            buf,
            "impl {}ClientExt for twirp::client::TwirpClient {{",
            service_name
        )
        .unwrap();
        for m in &service.methods {
            writeln!(
                buf,
                "    async fn {}_with_url(&self, url: twirp::url::Url, req: {}) -> Result<{}, twirp::client::TwirpClientError> {{",
                m.name, m.input_type, m.output_type,
            )
            .unwrap();
            writeln!(
                buf,
                "    twirp::client::request(self.client.post(url), req).await"
            )
            .unwrap();
            writeln!(buf, "}}").unwrap();
        }
        writeln!(buf, "}}").unwrap();
    }
}
