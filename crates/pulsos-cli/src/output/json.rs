use anyhow::Result;
use pulsos_core::domain::deployment::DeploymentEvent;

pub fn render(events: &[DeploymentEvent]) -> Result<()> {
    let json = serde_json::to_string_pretty(events)?;
    println!("{json}");
    Ok(())
}
