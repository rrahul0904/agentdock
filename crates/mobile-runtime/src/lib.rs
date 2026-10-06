use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

pub const PROFILE_SCHEMA_V1: &str = "agentdock-mobile-runtime-profile/v1";
pub const SESSION_SCHEMA_V1: &str = "agentdock-mobile-session-binding/v1";
pub const VOICE_SCHEMA_V1: &str = "agentdock-voice-ingress/v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeKind {
    NativeAndroid,
    TermuxProot,
    Remote,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum IsolationStrength {
    AppSandbox,
    ProotUserland,
    RemoteSandbox,
    Unknown,
}

impl IsolationStrength {
    pub fn is_strong_process_isolation(self) -> bool {
        matches!(self, Self::RemoteSandbox)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeCapability {
    FilesystemRead,
    FilesystemWrite,
    Git,
    TerminalProcess,
    AgentAppServer,
    VoiceIngress,
    BackgroundExecution,
    PreviewServer,
    AndroidBuild,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CapabilityState {
    Supported,
    Degraded,
    Unavailable,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceSource {
    DeviceProbe,
    RuntimeReceipt,
    OfficialDocumentation,
    UserDeclaration,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeHealthAssessment {
    pub capability: RuntimeCapability,
    pub state: CapabilityState,
    pub evidence_source: EvidenceSource,
    pub reason: String,
}

impl RuntimeHealthAssessment {
    pub fn unknown(capability: RuntimeCapability) -> Self {
        Self {
            capability,
            state: CapabilityState::Unknown,
            evidence_source: EvidenceSource::None,
            reason: "no runtime evidence supplied".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MobileRuntimeProfile {
    pub schema_version: String,
    pub runtime_id: String,
    pub platform: String,
    pub architecture: String,
    pub android_api_level: Option<u32>,
    pub kind: RuntimeKind,
    pub workspace_root: PathBuf,
    pub isolation: IsolationStrength,
    pub capabilities: Vec<RuntimeHealthAssessment>,
}

impl MobileRuntimeProfile {
    pub fn new(
        runtime_id: impl Into<String>,
        platform: impl Into<String>,
        architecture: impl Into<String>,
        android_api_level: Option<u32>,
        kind: RuntimeKind,
        workspace_root: PathBuf,
        isolation: IsolationStrength,
        capabilities: Vec<RuntimeHealthAssessment>,
    ) -> Result<Self, MobileRuntimeError> {
        let profile = Self {
            schema_version: PROFILE_SCHEMA_V1.to_string(),
            runtime_id: runtime_id.into(),
            platform: platform.into(),
            architecture: architecture.into(),
            android_api_level,
            kind,
            workspace_root,
            isolation,
            capabilities,
        };
        profile.validate()?;
        Ok(profile)
    }

    pub fn validate(&self) -> Result<(), MobileRuntimeError> {
        if self.schema_version != PROFILE_SCHEMA_V1 {
            return Err(MobileRuntimeError::SchemaVersionMismatch);
        }
        validate_identifier("runtime_id", &self.runtime_id)?;
        validate_nonempty("platform", &self.platform)?;
        validate_nonempty("architecture", &self.architecture)?;
        if self.workspace_root.as_os_str().is_empty() {
            return Err(MobileRuntimeError::EmptyWorkspaceRoot);
        }

        if matches!(
            self.kind,
            RuntimeKind::NativeAndroid | RuntimeKind::TermuxProot
        ) && !self.platform.eq_ignore_ascii_case("android")
        {
            return Err(MobileRuntimeError::AndroidRuntimeRequiresAndroidPlatform);
        }

        if matches!(
            self.kind,
            RuntimeKind::NativeAndroid | RuntimeKind::TermuxProot
        ) && self.isolation.is_strong_process_isolation()
        {
            return Err(MobileRuntimeError::UnsupportedIsolationClaim);
        }

        if matches!(
            self.kind,
            RuntimeKind::NativeAndroid | RuntimeKind::TermuxProot
        ) && self.android_api_level.is_none()
        {
            return Err(MobileRuntimeError::MissingAndroidApiLevel);
        }

        for assessment in &self.capabilities {
            validate_nonempty("capability reason", &assessment.reason)?;
        }

        Ok(())
    }

    pub fn health_for(&self, capability: RuntimeCapability) -> RuntimeHealthAssessment {
        self.capabilities
            .iter()
            .find(|assessment| assessment.capability == capability)
            .cloned()
            .unwrap_or_else(|| RuntimeHealthAssessment::unknown(capability))
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionMode {
    Local,
    Remote,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum MobileSessionState {
    Starting,
    Ready,
    Working,
    NeedsAttention,
    Reconnecting,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MobileSessionBinding {
    pub schema_version: String,
    pub owner_id: String,
    pub project_id: String,
    pub workspace_id: String,
    pub runtime_id: String,
    pub session_id: String,
    pub execution_mode: ExecutionMode,
    pub state: MobileSessionState,
}

impl MobileSessionBinding {
    pub fn new(
        owner_id: impl Into<String>,
        project_id: impl Into<String>,
        workspace_id: impl Into<String>,
        runtime_id: impl Into<String>,
        session_id: impl Into<String>,
        execution_mode: ExecutionMode,
    ) -> Result<Self, MobileRuntimeError> {
        let binding = Self {
            schema_version: SESSION_SCHEMA_V1.to_string(),
            owner_id: owner_id.into(),
            project_id: project_id.into(),
            workspace_id: workspace_id.into(),
            runtime_id: runtime_id.into(),
            session_id: session_id.into(),
            execution_mode,
            state: MobileSessionState::Starting,
        };
        binding.validate()?;
        Ok(binding)
    }

    pub fn validate(&self) -> Result<(), MobileRuntimeError> {
        if self.schema_version != SESSION_SCHEMA_V1 {
            return Err(MobileRuntimeError::SchemaVersionMismatch);
        }
        validate_identifier("owner_id", &self.owner_id)?;
        validate_identifier("project_id", &self.project_id)?;
        validate_identifier("workspace_id", &self.workspace_id)?;
        validate_identifier("runtime_id", &self.runtime_id)?;
        validate_identifier("session_id", &self.session_id)?;
        Ok(())
    }

    pub fn transition_to(&mut self, next: MobileSessionState) -> Result<(), MobileRuntimeError> {
        if self.state == next {
            return Ok(());
        }
        if !is_valid_transition(self.state, next) {
            return Err(MobileRuntimeError::InvalidSessionTransition {
                from: self.state,
                to: next,
            });
        }
        self.state = next;
        Ok(())
    }
}

fn is_valid_transition(from: MobileSessionState, to: MobileSessionState) -> bool {
    use MobileSessionState::*;
    match from {
        Starting => matches!(to, Ready | Failed | Stopped),
        Ready => matches!(to, Working | Reconnecting | Stopped | Failed),
        Working => matches!(to, Ready | NeedsAttention | Reconnecting | Stopped | Failed),
        NeedsAttention => matches!(to, Working | Reconnecting | Stopped | Failed),
        Reconnecting => matches!(to, Ready | Failed | Stopped),
        Stopped | Failed => false,
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum VoiceProvenance {
    OnDevice,
    Provider,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoiceIngressEnvelope {
    pub schema_version: String,
    pub owner_id: String,
    pub session_id: String,
    pub transcript: String,
    pub provenance: VoiceProvenance,
    submit: bool,
}

impl VoiceIngressEnvelope {
    pub fn new(
        owner_id: impl Into<String>,
        session_id: impl Into<String>,
        transcript: impl Into<String>,
        provenance: VoiceProvenance,
    ) -> Result<Self, MobileRuntimeError> {
        let envelope = Self {
            schema_version: VOICE_SCHEMA_V1.to_string(),
            owner_id: owner_id.into(),
            session_id: session_id.into(),
            transcript: transcript.into(),
            provenance,
            submit: false,
        };
        envelope.validate()?;
        Ok(envelope)
    }

    pub fn validate(&self) -> Result<(), MobileRuntimeError> {
        if self.schema_version != VOICE_SCHEMA_V1 {
            return Err(MobileRuntimeError::SchemaVersionMismatch);
        }
        validate_identifier("owner_id", &self.owner_id)?;
        validate_identifier("session_id", &self.session_id)?;
        validate_nonempty("transcript", &self.transcript)?;
        Ok(())
    }

    pub fn validate_for(&self, session: &MobileSessionBinding) -> Result<(), MobileRuntimeError> {
        self.validate()?;
        session.validate()?;
        if self.owner_id != session.owner_id || self.session_id != session.session_id {
            return Err(MobileRuntimeError::VoiceBindingMismatch);
        }
        Ok(())
    }

    pub fn submit_requested(&self) -> bool {
        self.submit
    }

    pub fn request_submit(&mut self) {
        self.submit = true;
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MobileRuntimeError {
    #[error("schema version does not match the supported contract")]
    SchemaVersionMismatch,
    #[error("{field} must not be empty")]
    EmptyField { field: &'static str },
    #[error("{field} is not a valid bounded identifier")]
    InvalidIdentifier { field: &'static str },
    #[error("workspace root must not be empty")]
    EmptyWorkspaceRoot,
    #[error("android runtime kinds require platform=android")]
    AndroidRuntimeRequiresAndroidPlatform,
    #[error("android runtime kinds require an Android API level")]
    MissingAndroidApiLevel,
    #[error("local Android/PRoot runtime cannot claim remote-sandbox isolation")]
    UnsupportedIsolationClaim,
    #[error("invalid mobile session transition from {from:?} to {to:?}")]
    InvalidSessionTransition {
        from: MobileSessionState,
        to: MobileSessionState,
    },
    #[error("voice ingress owner/session binding does not match target session")]
    VoiceBindingMismatch,
}

fn validate_nonempty(field: &'static str, value: &str) -> Result<(), MobileRuntimeError> {
    if value.trim().is_empty() {
        return Err(MobileRuntimeError::EmptyField { field });
    }
    Ok(())
}

fn validate_identifier(field: &'static str, value: &str) -> Result<(), MobileRuntimeError> {
    validate_nonempty(field, value)?;
    if value.len() > 128 || value.chars().any(char::is_control) {
        return Err(MobileRuntimeError::InvalidIdentifier { field });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proot_profile(capabilities: Vec<RuntimeHealthAssessment>) -> MobileRuntimeProfile {
        MobileRuntimeProfile::new(
            "pixel-runtime",
            "android",
            "arm64-v8a",
            Some(36),
            RuntimeKind::TermuxProot,
            PathBuf::from("/data/user/0/com.example/files/workspace"),
            IsolationStrength::ProotUserland,
            capabilities,
        )
        .expect("valid proot profile")
    }

    #[test]
    fn proot_cannot_claim_remote_sandbox_isolation() {
        let error = MobileRuntimeProfile::new(
            "pixel-runtime",
            "android",
            "arm64-v8a",
            Some(36),
            RuntimeKind::TermuxProot,
            PathBuf::from("/data/user/0/com.example/files/workspace"),
            IsolationStrength::RemoteSandbox,
            vec![],
        )
        .expect_err("strong isolation claim must be refused");

        assert_eq!(error, MobileRuntimeError::UnsupportedIsolationClaim);
        assert!(!IsolationStrength::ProotUserland.is_strong_process_isolation());
    }

    #[test]
    fn absent_capability_evidence_stays_unknown() {
        let profile = proot_profile(vec![]);
        let health = profile.health_for(RuntimeCapability::AgentAppServer);

        assert_eq!(health.state, CapabilityState::Unknown);
        assert_eq!(health.evidence_source, EvidenceSource::None);
    }

    #[test]
    fn explicit_capability_evidence_is_preserved() {
        let expected = RuntimeHealthAssessment {
            capability: RuntimeCapability::Git,
            state: CapabilityState::Degraded,
            evidence_source: EvidenceSource::DeviceProbe,
            reason: "git available but background execution is not certified".to_string(),
        };
        let profile = proot_profile(vec![expected.clone()]);

        assert_eq!(profile.health_for(RuntimeCapability::Git), expected);
    }

    #[test]
    fn session_state_machine_fails_closed() {
        let mut session = MobileSessionBinding::new(
            "owner-1",
            "project-1",
            "workspace-1",
            "pixel-runtime",
            "session-1",
            ExecutionMode::Local,
        )
        .expect("valid binding");

        let error = session
            .transition_to(MobileSessionState::NeedsAttention)
            .expect_err("starting cannot jump directly to needs-attention");
        assert_eq!(
            error,
            MobileRuntimeError::InvalidSessionTransition {
                from: MobileSessionState::Starting,
                to: MobileSessionState::NeedsAttention,
            }
        );

        session.transition_to(MobileSessionState::Ready).unwrap();
        session.transition_to(MobileSessionState::Working).unwrap();
        session
            .transition_to(MobileSessionState::NeedsAttention)
            .unwrap();
        session.transition_to(MobileSessionState::Stopped).unwrap();
        assert!(session.transition_to(MobileSessionState::Ready).is_err());
    }

    #[test]
    fn voice_ingress_is_draft_only_by_default() {
        let mut voice = VoiceIngressEnvelope::new(
            "owner-1",
            "session-1",
            "please run the tests",
            VoiceProvenance::OnDevice,
        )
        .expect("valid voice envelope");

        assert!(!voice.submit_requested());
        voice.request_submit();
        assert!(voice.submit_requested());
    }

    #[test]
    fn voice_ingress_must_match_owner_and_session() {
        let session = MobileSessionBinding::new(
            "owner-1",
            "project-1",
            "workspace-1",
            "pixel-runtime",
            "session-1",
            ExecutionMode::Local,
        )
        .unwrap();
        let voice = VoiceIngressEnvelope::new(
            "owner-2",
            "session-1",
            "continue",
            VoiceProvenance::Provider,
        )
        .unwrap();

        assert_eq!(
            voice.validate_for(&session),
            Err(MobileRuntimeError::VoiceBindingMismatch)
        );
    }

    #[test]
    fn execution_mode_serialization_is_stable() {
        assert_eq!(
            serde_json::to_string(&ExecutionMode::Local).unwrap(),
            "\"local\""
        );
        assert_eq!(
            serde_json::to_string(&ExecutionMode::Remote).unwrap(),
            "\"remote\""
        );
    }
}
