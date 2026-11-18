/// Declarative compatibility capabilities supported by a model.
///
/// Capabilities describe supported inputs, outputs, and interactions. They are
/// provider/model classifications used to match an agent's requirements; they
/// are not standardized guarantees or measurements of model quality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize)]
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

    /// Classified as supporting multi-step reasoning capabilities.
    ///
    /// This is a provider/model classification and does not imply a uniform
    /// reasoning level or quality guarantee.
    Reasoning,
}
