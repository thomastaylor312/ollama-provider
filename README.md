# Ollama Capability Provider

This capability provider is an implementation of the `thomastaylor312:ollama` interface. It exposes the Ollama API to components.

## Link Configuration

To configure this provider, use the following configuration values as `target_config` in the link:

| Property     | Description                                                                          |
| :----------- | :----------------------------------------------------------------------------------- |
| `model_name` | The name of the model to use for requests                                            |
| `url`        | The URL of the Ollama API. If not specified, the default is `http://localhost:11434` |

## Caveats

Currently wasmCloud doesn't support resources in custom interfaces. The support for doing this just landed in upstream wasmtime and should be added soon, which will make this interface better. For now, it is highly recommended to `ollama pull` your desired models and then set your RPC timeouts high on your hosts (30-60s)
