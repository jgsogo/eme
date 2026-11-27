//! The fixed operation bound to each registered Home Assistant tool.

/// Home Assistant operation implemented by a tool instance.
#[derive(Clone, Copy)]
pub enum Operation {
    GetStates,
    GetState,
    GetServices,
    CallService,
    GetHistory,
    GetConfig,
}
