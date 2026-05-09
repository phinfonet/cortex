pub mod client;
pub mod protocol;
pub mod server;

pub use client::UiClient;
pub use protocol::{AppEventDto, ApprovalKindDto, DaemonMessage, TuiMessage};
#[allow(unused_imports)]
pub use server::{ApprovalHandle, UiServer};
