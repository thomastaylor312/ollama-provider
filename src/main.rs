mod ollama;

use ollama::OllamaProvider;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    OllamaProvider::run().await?;
    eprintln!("Ollama provider exiting");
    Ok(())
}
