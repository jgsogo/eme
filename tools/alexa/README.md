# alexa-tool

## Purpose

The `alexa-tool` crate provides the platform's collection of tools for interacting with Amazon Alexa. It is the home for Alexa-related capabilities exposed to agents through the platform's tool interface.

The crate currently implements one tool, `alexa_speak`. Additional Alexa tools will be added here as the platform's needs evolve; the crate is intentionally not scoped to text-to-speech alone.

## Responsibilities

The crate owns Alexa-specific tool implementations and the integration code they need, including:

- **Tool implementations:** implement each Alexa capability as an independently registered tool with a model-facing definition and argument schema.
- **Alexa service integration:** encapsulate the Alexa-specific authentication, session handling, API/protocol interactions, and response handling required by its tools.
- **Device discovery and resolution:** provide shared functionality for locating Alexa devices and resolving the identifiers required by individual operations.
- **Configuration boundary:** read credentials, device configuration, and other integration settings from the environment or tool configuration, without exposing secrets as model-controlled arguments.
- **Error reporting:** translate Alexa service and integration failures into useful tool errors for the runtime.

As of now, `alexa_speak` is the first available tool. It sends speech to an Alexa device. Other Alexa-related operations belong in this crate as they are implemented.

## Boundaries

- `alexa-tool` owns Alexa-specific integration code and the implementations of all Alexa-related tools.
- `agent-runtime` owns tool registration infrastructure, argument validation, authorization, dispatch, and invocation.
- Agent-facing tool definitions describe the operations and their accepted arguments. Credentials and other privileged configuration remain outside model-controlled arguments.
- Shared Alexa integration code should be reused by tools where appropriate, rather than duplicating authentication or device-resolution logic in each tool.

## Non-goals

This crate does not own agent execution, scheduling, conversation management, or authorization decisions. It is also not intended to be a general-purpose Alexa SDK: its public purpose is to provide the set of Alexa capabilities that the agent platform exposes as tools.
