set dotenv-load
set positional-arguments

# Default recipe that runs when you just type `just`
default:
    just --list

# Run the Rust project with environment variables loaded
model-playground *args:
    cargo run -p agent-models --bin model-playground -- --data-dir "{{justfile_directory()}}/data" "$@"

model-gemini *args: (model-playground "--model" "gemini/gemini-3.5-flash-lite" args)
model-groq *args: (model-playground "--model" "groq/openai/gpt-oss-20b" args)
model-cloudfare *args: (model-playground "--model" "cloudflare/@cf/meta/llama-3.1-8b-instruct-fp8" args)


ex-basic-agent-gemini:
    cargo run -p agent-runtime --example basic_agent -- gemini/gemini-3.5-flash-lite "{{justfile_directory()}}/data"

ex-basic-agent-groq:
    cargo run -p agent-runtime --example basic_agent -- groq/openai/gpt-oss-20b "{{justfile_directory()}}/data"

ex-basic-agent-cf:
    # cargo run -p agent-runtime --example basic_agent -- cloudflare/@cf/meta/llama-3.1-8b-instruct-fp8 "{{justfile_directory()}}/data"
    cargo run -p agent-runtime --example basic_agent -- cloudflare/@cf/zai-org/glm-4.7-flash "{{justfile_directory()}}/data"

azn-tts device speech:
    @echo "Amazon TTS to device '{{ device }}': {{ speech }}"
    /home/jgsogo/github/alexa-remote-control/alexa_remote_control.sh -d "{{ device }}" -e "speak:{{ speech }}"

azn-totoro-echo speech: (azn-tts "Totoro Echo" speech)

basic_agent:
    cargo run -p agent-runtime --example basic_agent