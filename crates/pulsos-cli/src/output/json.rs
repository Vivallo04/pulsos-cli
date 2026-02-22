use anyhow::Result;
use pulsos_core::domain::project::CorrelatedEvent;

pub fn render_correlated_with_health(
    events: &[CorrelatedEvent],
    health_scores: &[(String, u8)],
) -> Result<()> {
    let health: Vec<serde_json::Value> = health_scores
        .iter()
        .map(|(name, score)| serde_json::json!({"project": name, "score": score}))
        .collect();

    let output = serde_json::json!({
        "events": events,
        "health": health,
    });
    let json = serde_json::to_string_pretty(&output)?;
    println!("{json}");
    Ok(())
}
