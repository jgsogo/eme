//! Validated options accepted by the web-search tool.

mod arguments;
pub(crate) use arguments::Arguments;
mod search_depth;
pub(crate) use search_depth::SearchDepth;
mod search_topic;
pub(crate) use search_topic::SearchTopic;
mod time_range;
pub(crate) use time_range::TimeRange;
