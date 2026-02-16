use chrono::{DateTime, Utc};
use pulsos_core::domain::deployment::{DeploymentEvent, DeploymentStatus, EventMetadata, Platform};

/// Builder for constructing test `DeploymentEvent` instances.
pub struct EventBuilder {
    event: DeploymentEvent,
}

impl EventBuilder {
    pub fn new() -> Self {
        Self {
            event: DeploymentEvent {
                id: "test-001".into(),
                platform: Platform::GitHub,
                status: DeploymentStatus::Success,
                commit_sha: Some("abc123".into()),
                branch: Some("main".into()),
                title: Some("Test deployment".into()),
                actor: Some("testuser".into()),
                created_at: Utc::now(),
                updated_at: None,
                duration_secs: None,
                url: None,
                metadata: EventMetadata::default(),
            },
        }
    }

    pub fn id(mut self, id: &str) -> Self {
        self.event.id = id.into();
        self
    }

    pub fn platform(mut self, platform: Platform) -> Self {
        self.event.platform = platform;
        self
    }

    pub fn status(mut self, status: DeploymentStatus) -> Self {
        self.event.status = status;
        self
    }

    pub fn commit_sha(mut self, sha: &str) -> Self {
        self.event.commit_sha = Some(sha.into());
        self
    }

    pub fn branch(mut self, branch: &str) -> Self {
        self.event.branch = Some(branch.into());
        self
    }

    pub fn title(mut self, title: &str) -> Self {
        self.event.title = Some(title.into());
        self
    }

    pub fn actor(mut self, actor: &str) -> Self {
        self.event.actor = Some(actor.into());
        self
    }

    pub fn created_at(mut self, dt: DateTime<Utc>) -> Self {
        self.event.created_at = dt;
        self
    }

    pub fn duration(mut self, secs: u64) -> Self {
        self.event.duration_secs = Some(secs);
        self
    }

    pub fn url(mut self, url: &str) -> Self {
        self.event.url = Some(url.into());
        self
    }

    pub fn workflow_name(mut self, name: &str) -> Self {
        self.event.metadata.workflow_name = Some(name.into());
        self
    }

    pub fn build(self) -> DeploymentEvent {
        self.event
    }
}

impl Default for EventBuilder {
    fn default() -> Self {
        Self::new()
    }
}
