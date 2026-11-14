# alexa-tool

## Purpose

The `alexa-tool` crate will provide an Alexa text-to-speech capability that agents can invoke through the platform's tool interface.

It will encapsulate the Alexa-specific authentication, device discovery, and speech-command interaction behind a tool implementation, while exposing a model-facing tool definition that describes the input expected from an agent.

## Responsibilities

The crate will implement:

- **Tool integration:** an implementation of the agent runtime's tool contract that will make Alexa speech available as an agent-invocable capability.
- **Tool definition:** metadata and a JSON Schema describing the speech operation and its accepted arguments.
- **Alexa authentication:** the authentication flow required to obtain an Alexa session from the configured refresh-token credential.
- **Device resolution:** locating the configured Echo device and resolving the identifiers required by the Alexa service.
- **Speech command submission:** sending the requested text or supported speech markup to the selected device and reporting success or failure to the runtime.
- **Configuration boundary:** reading the required device configuration and credentials from the environment or tool configuration, without exposing those details to the model.

## Boundaries

- `alexa-tool` will own the Alexa-specific protocol and the implementation of this external capability.
- `agent-runtime` will own tool registration, argument validation, authorization, and invocation.
- Agent-facing tool metadata will describe the operation; credentials and device configuration will remain outside model-controlled arguments.

## Non-goals

This crate will not provide a general-purpose Alexa client, implement the full Alexa Remote Control feature set, manage agent execution, or make authorization decisions. Its scope will remain focused on the speech capability.
