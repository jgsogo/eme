set dotenv-load
set positional-arguments

# Default recipe that runs when you just type `just`
default:
    just --list

# Run the Rust project with environment variables loaded
model-playground *args:
    cargo run -p agent-models --bin model-playground -- "$@"

model-gemini *args: (model-playground "--model" "gemini/gemini-3.5-flash-lite" args)

model-groq *args: (model-playground "--model" "groq/gemini-3.5-flash-lite" args)

