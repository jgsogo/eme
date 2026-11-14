/// Capabilities supported by an LLM model.
///
/// These describe the kinds of input, output, and interactions a model
/// supports. They can be used to filter or select models based on the
/// requirements of a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelCapability {
    /// Conversational text generation and understanding.
    Chat,

    /// Code generation, explanation, debugging, and programming assistance.
    ///
    /// This is an application-level classification rather than a
    /// standardized API capability.
    Coding,

    /// Understanding visual inputs, such as images.
    Vision,

    /// Understanding video inputs and their temporal content.
    VideoUnderstanding,

    /// Generating images from text or other supported inputs.
    ImageGeneration,

    /// Converting speech or other audio input into text.
    Transcription,

    /// Understanding and reasoning about audio inputs beyond transcription.
    AudioUnderstanding,

    /// Generating spoken audio from text.
    TextToSpeech,

    /// Generating vector embeddings for text or other supported inputs.
    Embeddings,

    /// Invoking external functions or tools through model-generated
    /// tool calls.
    ToolCalling,

    /// Producing outputs constrained to a structured format, such as JSON
    /// or a JSON Schema.
    StructuredOutput,
}
