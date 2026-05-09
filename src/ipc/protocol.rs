use serde::{Deserialize, Serialize};

use crate::monitor::AppEvent;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DaemonMessage {
    Event {
        data: AppEventDto,
    },
    CommandOutput {
        lobe: String,
        text: String,
    },
    ApprovalRequest {
        id: String,
        title: String,
        body: String,
        approval_kind: ApprovalKindDto,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TuiMessage {
    ApprovalResponse { id: String, accepted: bool },
    Command { lobe: String, text: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppEventDto {
    FileChanged {
        lobe: String,
        path: String,
    },
    InquiryDetected {
        lobe: String,
        id: String,
        title: String,
    },
    InquiryStarted {
        lobe: String,
        id: String,
    },
    InquiryCompleted {
        lobe: String,
        id: String,
        output_path: String,
    },
    InquiryFailed {
        lobe: String,
        id: String,
        reason: String,
    },
    PlanDetected {
        lobe: String,
        filename: String,
    },
    PlanStarted {
        lobe: String,
        filename: String,
    },
    PlanCompleted {
        lobe: String,
        filename: String,
        #[serde(default)]
        summary: String,
        #[serde(default)]
        diff: Option<String>,
    },
    PlanFailed {
        lobe: String,
        filename: String,
        reason: String,
    },
    PlanNeedsPermission {
        lobe: String,
        filename: String,
    },
    CommandReceived {
        lobe: String,
        text: String,
    },
    CommandCompleted {
        lobe: String,
        output: String,
    },
    AgentStarted {
        description: String,
    },
    AgentCompleted {
        description: String,
    },
    SessionEnded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalKindDto {
    CodeReview,
    Permission,
}

impl From<AppEvent> for Option<AppEventDto> {
    fn from(event: AppEvent) -> Self {
        match event {
            AppEvent::FileChanged { lobe, path } => Some(AppEventDto::FileChanged { lobe, path }),
            AppEvent::InquiryDetected { lobe, inquiry } => Some(AppEventDto::InquiryDetected {
                lobe,
                id: inquiry.id,
                title: inquiry.title,
            }),
            AppEvent::InquiryStarted { lobe, id } => Some(AppEventDto::InquiryStarted { lobe, id }),
            AppEvent::InquiryCompleted {
                lobe,
                id,
                output_path,
            } => Some(AppEventDto::InquiryCompleted {
                lobe,
                id,
                output_path,
            }),
            AppEvent::InquiryFailed { lobe, id, reason } => {
                Some(AppEventDto::InquiryFailed { lobe, id, reason })
            }
            AppEvent::PlanDetected { lobe, plan } => Some(AppEventDto::PlanDetected {
                lobe,
                filename: plan.filename,
            }),
            AppEvent::PlanStarted { lobe, filename } => {
                Some(AppEventDto::PlanStarted { lobe, filename })
            }
            AppEvent::PlanCompleted {
                lobe,
                filename,
                summary,
                diff,
            } => Some(AppEventDto::PlanCompleted {
                lobe,
                filename,
                summary,
                diff,
            }),
            AppEvent::PlanFailed {
                lobe,
                filename,
                reason,
            } => Some(AppEventDto::PlanFailed {
                lobe,
                filename,
                reason,
            }),
            AppEvent::PlanNeedsPermission { lobe, filename } => {
                Some(AppEventDto::PlanNeedsPermission { lobe, filename })
            }
            AppEvent::CommandReceived { lobe, text } => {
                Some(AppEventDto::CommandReceived { lobe, text })
            }
            AppEvent::CommandCompleted { lobe, output } => {
                Some(AppEventDto::CommandCompleted { lobe, output })
            }
            AppEvent::AgentStarted { description } => {
                Some(AppEventDto::AgentStarted { description })
            }
            AppEvent::AgentCompleted { description } => {
                Some(AppEventDto::AgentCompleted { description })
            }
            AppEvent::SessionEnded => Some(AppEventDto::SessionEnded),
            AppEvent::HookRaw { .. } => None,
        }
    }
}
