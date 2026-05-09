#[derive(Debug, Clone)]
pub enum AppEvent {
    FileChanged {
        lobe: String,
        path: String,
    },
    InquiryDetected {
        lobe: String,
        inquiry: InquiryMeta,
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
        plan: PlanMeta,
    },
    PlanStarted {
        lobe: String,
        filename: String,
    },
    PlanCompleted {
        lobe: String,
        filename: String,
        summary: String,
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
    HookRaw {
        payload: serde_json::Value,
    },
}

#[derive(Debug, Clone)]
pub struct InquiryMeta {
    pub id: String,
    pub title: String,
    pub kind: InquiryKind,
    pub output: String,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct PlanMeta {
    pub filename: String,
    pub path: String,
    pub project: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InquiryKind {
    Research,
    Decision,
    Analysis,
}
