use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Context as _};
use ollama_rs::generation::completion::request::GenerationRequest;
use ollama_rs::generation::completion::GenerationResponse;
use ollama_rs::generation::images::Image;
use ollama_rs::Ollama;
use tokio::sync::RwLock;
use tracing::{debug, error};
use wasmcloud_provider_sdk::core::HostData;
use wasmcloud_provider_sdk::{
    get_connection, load_host_data, run_provider, Context, LinkConfig, Provider,
};

wit_bindgen_wrpc::generate!({
    additional_derives: [serde::Serialize, serde::Deserialize, Default],
});

use exports::thomastaylor312::ollama::generate::Handler;
use exports::thomastaylor312::ollama::generate::{Request, Response};

const MODEL_NAME_KEY: &str = "model_name";
const URL_KEY: &str = "url";

impl Request {
    fn into_generation_request(self, model: String) -> GenerationRequest {
        GenerationRequest::new(model, self.prompt).images(
            self.images
                .unwrap_or_default()
                .into_iter()
                .map(|s| Image::from_base64(&s))
                .collect(),
        )
    }
}

impl From<GenerationResponse> for Response {
    fn from(resp: GenerationResponse) -> Self {
        let mut r = Response {
            model: resp.model,
            created_at: resp.created_at,
            response: resp.response,
            done: resp.done,
            ..Default::default()
        };
        if let Some(data) = resp.final_data {
            r.context = Some(data.context.0);
            r.total_duration = Some(data.total_duration);
            r.prompt_eval_count = Some(data.prompt_eval_count);
            r.prompt_eval_duration = Some(data.prompt_eval_duration);
            r.eval_count = Some(data.eval_count);
            r.eval_duration = Some(data.eval_duration);
        }
        r
    }
}

/// Configuration for the Ollama component, gathered from configuration. Right now this isn't much
/// but we can expand with more features later.
#[derive(Debug, Clone)]
struct OllamaConfig {
    model: String,
    host: String,
    port: u16,
}

impl OllamaConfig {
    /// Merge another configuration into this one, preferring values from the other configuration.
    /// This is the equivalent of cloning and then manually updating fields
    fn merge(&self, config: &HashMap<String, String>) -> anyhow::Result<Self> {
        let (host, port) = if let Some(raw) = config.get("host") {
            get_host_and_port(raw)?
        } else {
            (self.host.clone(), self.port)
        };
        Ok(Self {
            model: config
                .get(MODEL_NAME_KEY)
                .cloned()
                .unwrap_or_else(|| self.model.clone()),
            host,
            port,
        })
    }
}

/// Ollama implementation for the ollama interface
#[derive(Clone)]
pub struct OllamaProvider {
    /// Map of NATS connection clients (including subscriptions) per component
    components: Arc<RwLock<HashMap<String, OllamaConfig>>>,
    /// Default configuration to use when configuration is not provided on the link
    default_config: OllamaConfig,
}

impl OllamaProvider {
    /// Execute the provider, loading default configuration from the host and subscribing
    /// on the proper RPC topics via `wrpc::serve`
    pub async fn run() -> anyhow::Result<()> {
        let host_data = load_host_data().context("failed to load host data")?;
        let provider = Self::from_host_data(host_data);
        let shutdown = run_provider(provider.clone(), "ollama-provider")
            .await
            .context("failed to run provider")?;
        let connection = get_connection();
        serve(
            &connection.get_wrpc_client(connection.provider_key()),
            provider,
            shutdown,
        )
        .await
    }

    /// Build a [`OllamaProvider`] from [`HostData`]
    pub fn from_host_data(host_data: &HostData) -> Self {
        // TODO: Be more friendly parsing upper vs lower case keys
        let (host, port) = if let Some(raw) = host_data.config.get(URL_KEY) {
            get_host_and_port(raw).unwrap_or_else(|e| {
                error!(err = %e, "Invalid host in config");
                ("http://localhost".to_string(), 11434)
            })
        } else {
            ("http://localhost".to_string(), 11434)
        };
        let default_config = OllamaConfig {
            model: host_data
                .config
                .get(MODEL_NAME_KEY)
                .map(|s| s.as_str())
                .unwrap_or("llama3")
                .to_string(),
            host,
            port,
        };
        OllamaProvider {
            default_config,
            components: Default::default(),
        }
    }
}

impl Provider for OllamaProvider {
    async fn receive_link_config_as_target(
        &self,
        LinkConfig {
            source_id, config, ..
        }: LinkConfig<'_>,
    ) -> anyhow::Result<()> {
        let config = if config.is_empty() {
            self.default_config.clone()
        } else {
            self.default_config.merge(config)?
        };

        self.components
            .write()
            .await
            .insert(source_id.into(), config);

        Ok(())
    }

    /// Handle notification that a link is dropped: close the connection which removes all subscriptions
    async fn delete_link(&self, source_id: &str) -> anyhow::Result<()> {
        self.components.write().await.remove(source_id);

        debug!(
            component_id = %source_id,
            "finished processing delete link for component",
        );
        Ok(())
    }

    /// Handle shutdown request by closing all connections
    async fn shutdown(&self) -> anyhow::Result<()> {
        let mut all_components = self.components.write().await;
        all_components.clear();
        Ok(())
    }
}

/// Implement the 'wasmcloud:messaging' capability provider interface
impl Handler<Option<Context>> for OllamaProvider {
    async fn generate(
        &self,
        ctx: Option<Context>,
        req: Request,
    ) -> anyhow::Result<Result<Response, String>> {
        let ctx = ctx.ok_or_else(|| anyhow::anyhow!("no context provided"))?;
        let (ollama, model_name) = {
            let components = self.components.read().await;
            let component_id = ctx
                .component
                .as_ref()
                .ok_or_else(|| anyhow!("Context is missing component ID"))?;
            let conf = components
                .get(component_id)
                .ok_or_else(|| anyhow::anyhow!("component is not linked: {}", component_id))?;
            (
                Ollama::new(conf.host.clone(), conf.port),
                conf.model.clone(),
            )
        };
        match ollama
            .generate(req.into_generation_request(model_name))
            .await
        {
            Ok(resp) => Ok(Ok(resp.into())),
            Err(e) => Ok(Err(format!("Error generating: {}", e))),
        }
    }
}

fn get_host_and_port(raw: &str) -> anyhow::Result<(String, u16)> {
    let url = url::Url::parse(raw).context("Invalid host URL")?;
    Ok((
        format!(
            "{}://{}",
            url.scheme(),
            url.host_str()
                .ok_or_else(|| anyhow!("Given URL didn't have a host set"))?
        ),
        url.port_or_known_default()
            .ok_or_else(|| anyhow!("Unable to ascertain port from host URL"))?,
    ))
}
