use pulsos_core::auth::PlatformKind;
use pulsos_core::platform::DiscoveredResource;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsFlowState {
    Idle,
    ProviderActions,
    TokenEntry,
    ValidatingToken,
    ValidationResult,
    DiscoveryScanning,
    ResourceSelection,
    CorrelationReview,
    Applying,
}

#[derive(Debug, Clone)]
pub struct SelectableResource {
    pub resource: DiscoveredResource,
    pub selected: bool,
}

#[derive(Debug, Clone)]
pub struct SelectableVercelResource {
    pub resource: DiscoveredResource,
    pub linked_repo: Option<String>,
    pub selected: bool,
}

#[derive(Debug, Clone, Default)]
pub struct DiscoveryPayload {
    pub github: Vec<DiscoveredResource>,
    pub railway: Vec<DiscoveredResource>,
    pub vercel: Vec<(DiscoveredResource, Option<String>)>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct OnboardingState {
    pub platform_cursor: usize,
    pub platform_selected: Vec<bool>,
    pub resource_cursor: usize,
    pub github: Vec<SelectableResource>,
    pub railway: Vec<SelectableResource>,
    pub vercel: Vec<SelectableVercelResource>,
    pub correlation_preview: Vec<String>,
}

impl Default for OnboardingState {
    fn default() -> Self {
        Self {
            platform_cursor: 0,
            platform_selected: vec![false; PlatformKind::ALL.len()],
            resource_cursor: 0,
            github: vec![],
            railway: vec![],
            vercel: vec![],
            correlation_preview: vec![],
        }
    }
}

impl OnboardingState {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn selected_platforms(&self) -> Vec<PlatformKind> {
        PlatformKind::ALL
            .iter()
            .enumerate()
            .filter_map(|(i, p)| self.platform_selected.get(i).copied().unwrap_or(false).then_some(*p))
            .collect()
    }

    pub fn set_discovery(&mut self, payload: DiscoveryPayload) {
        self.github = payload
            .github
            .into_iter()
            .map(|resource| SelectableResource {
                resource,
                selected: false,
            })
            .collect();
        self.railway = payload
            .railway
            .into_iter()
            .map(|resource| SelectableResource {
                resource,
                selected: false,
            })
            .collect();
        self.vercel = payload
            .vercel
            .into_iter()
            .map(|(resource, linked_repo)| SelectableVercelResource {
                resource,
                linked_repo,
                selected: false,
            })
            .collect();
        self.resource_cursor = 0;
        self.correlation_preview.clear();
    }

    pub fn selectable_count(&self) -> usize {
        self.github.len() + self.railway.len() + self.vercel.len()
    }

    pub fn selected_count(&self) -> usize {
        self.github.iter().filter(|r| r.selected).count()
            + self.railway.iter().filter(|r| r.selected).count()
            + self.vercel.iter().filter(|r| r.selected).count()
    }

    pub fn clamp_resource_cursor(&mut self) {
        let max = self.selectable_count();
        if max == 0 {
            self.resource_cursor = 0;
        } else if self.resource_cursor >= max {
            self.resource_cursor = max - 1;
        }
    }

    pub fn toggle_resource(&mut self, flat_index: usize) {
        let mut idx = flat_index;
        if idx < self.github.len() {
            self.github[idx].selected = !self.github[idx].selected;
            return;
        }
        idx -= self.github.len();
        if idx < self.railway.len() {
            self.railway[idx].selected = !self.railway[idx].selected;
            return;
        }
        idx -= self.railway.len();
        if idx < self.vercel.len() {
            self.vercel[idx].selected = !self.vercel[idx].selected;
        }
    }

    pub fn selected_discovery(&self) -> DiscoveryPayload {
        DiscoveryPayload {
            github: self
                .github
                .iter()
                .filter(|item| item.selected)
                .map(|item| item.resource.clone())
                .collect(),
            railway: self
                .railway
                .iter()
                .filter(|item| item.selected)
                .map(|item| item.resource.clone())
                .collect(),
            vercel: self
                .vercel
                .iter()
                .filter(|item| item.selected)
                .map(|item| (item.resource.clone(), item.linked_repo.clone()))
                .collect(),
            warnings: Vec::new(),
        }
    }
}
