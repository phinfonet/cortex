#[derive(Debug, Clone)]
pub enum AppEvent {
    FileChanged { lobe: String, path: String },
    InquiryDetected { lobe: String, inquiry: InquiryMeta },
    InquiryStarted { lobe: String, id: String },
    InquiryCompleted { lobe: String, id: String, output_path: String },
    InquiryFailed { lobe: String, id: String, reason: String },
    AgentStarted { description: String },
    AgentCompleted { description: String },
    SessionEnded,
    HookRaw { payload: serde_json::Value },
}

#[derive(Debug, Clone)]
pub struct InquiryMeta {
    pub id: String,
    pub title: String,
    pub kind: InquiryKind,
    pub output: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InquiryKind {
    Research,
    Decision,
    Analysis,
}
