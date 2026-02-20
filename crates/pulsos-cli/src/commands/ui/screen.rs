use std::io::{IsTerminal, Write};

use anyhow::Result;
use crossterm::{
    cursor,
    execute,
    terminal::{Clear, ClearType},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenSeverity {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct ScreenSpec {
    pub title: String,
    pub subtitle: Option<String>,
    pub step_index: Option<usize>,
    pub step_total: Option<usize>,
    pub body_lines: Vec<String>,
    pub hints: Vec<String>,
    pub severity: ScreenSeverity,
}

impl ScreenSpec {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
            step_index: None,
            step_total: None,
            body_lines: vec![],
            hints: vec![],
            severity: ScreenSeverity::Info,
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn step(mut self, index: usize, total: usize) -> Self {
        self.step_index = Some(index);
        self.step_total = Some(total);
        self
    }

    pub fn body_lines<I, S>(mut self, lines: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.body_lines = lines.into_iter().map(Into::into).collect();
        self
    }

    pub fn hints<I, S>(mut self, hints: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.hints = hints.into_iter().map(Into::into).collect();
        self
    }

    pub fn severity(mut self, severity: ScreenSeverity) -> Self {
        self.severity = severity;
        self
    }
}

#[derive(Debug, Clone)]
pub struct PromptResult<T> {
    pub value: Option<T>,
    pub cancelled: bool,
}

impl<T> PromptResult<T> {
    fn value(value: T) -> Self {
        Self {
            value: Some(value),
            cancelled: false,
        }
    }

    fn cancelled() -> Self {
        Self {
            value: None,
            cancelled: true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ScreenSession {
    interactive: bool,
    width: usize,
}

impl ScreenSession {
    pub fn new() -> Self {
        Self {
            interactive: std::io::stdout().is_terminal(),
            width: 76,
        }
    }

    pub fn render(&self, spec: &ScreenSpec) -> Result<()> {
        if self.interactive {
            let mut out = std::io::stdout();
            execute!(out, Clear(ClearType::All), cursor::MoveTo(0, 0))?;
        }
        print!("{}", format_screen(spec, self.width));
        std::io::stdout().flush()?;
        Ok(())
    }

    pub fn render_with_prompt(&self, spec: &ScreenSpec, prompt: &str) -> Result<()> {
        let mut spec = spec.clone();
        spec.body_lines.push(String::new());
        spec.body_lines.push(format!("Prompt: {prompt}"));
        self.render(&spec)
    }
}

pub fn screen_confirm(
    session: &ScreenSession,
    spec: &ScreenSpec,
    prompt: &str,
    default: bool,
) -> Result<PromptResult<bool>> {
    session.render_with_prompt(spec, prompt)?;
    match dialoguer::Confirm::new()
        .with_prompt(prompt)
        .default(default)
        .interact_opt()
    {
        Ok(Some(value)) => {
            render_result(
                session,
                spec,
                ScreenSeverity::Success,
                format!("{} {}", if value { "✓" } else { "○" }, prompt),
            )?;
            Ok(PromptResult::value(value))
        }
        Ok(None) => {
            render_result(
                session,
                spec,
                ScreenSeverity::Warning,
                "Cancelled by user.".to_string(),
            )?;
            Ok(PromptResult::cancelled())
        }
        Err(e) if is_cancelled_error(&e.to_string()) => {
            render_result(
                session,
                spec,
                ScreenSeverity::Warning,
                "Cancelled by user.".to_string(),
            )?;
            Ok(PromptResult::cancelled())
        }
        Err(e) => Err(e.into()),
    }
}

pub fn screen_input(
    session: &ScreenSession,
    spec: &ScreenSpec,
    prompt: &str,
    initial: Option<&str>,
    allow_empty: bool,
) -> Result<PromptResult<String>> {
    session.render_with_prompt(spec, prompt)?;
    let mut input = dialoguer::Input::<String>::new().with_prompt(prompt);
    if let Some(initial) = initial {
        input = input.with_initial_text(initial);
    }
    if allow_empty {
        input = input.allow_empty(true);
    }

    match input.interact() {
        Ok(value) => {
            render_result(
                session,
                spec,
                ScreenSeverity::Success,
                "Input captured.".to_string(),
            )?;
            Ok(PromptResult::value(value))
        }
        Err(e) if is_cancelled_error(&e.to_string()) => {
            render_result(
                session,
                spec,
                ScreenSeverity::Warning,
                "Cancelled by user.".to_string(),
            )?;
            Ok(PromptResult::cancelled())
        }
        Err(e) => Err(e.into()),
    }
}

pub fn screen_password_masked<F>(
    session: &ScreenSession,
    spec: &ScreenSpec,
    prompt: &str,
    read_fn: F,
) -> Result<PromptResult<String>>
where
    F: Fn(&str) -> Result<String>,
{
    session.render_with_prompt(spec, prompt)?;
    match read_fn(prompt) {
        Ok(value) => {
            render_result(
                session,
                spec,
                ScreenSeverity::Success,
                "Input captured.".to_string(),
            )?;
            Ok(PromptResult::value(value))
        }
        Err(e) if is_cancelled_error(&e.to_string()) => {
            render_result(
                session,
                spec,
                ScreenSeverity::Warning,
                "Cancelled by user.".to_string(),
            )?;
            Ok(PromptResult::cancelled())
        }
        Err(e) => Err(e),
    }
}

pub fn screen_select<S: Clone + ToString>(
    session: &ScreenSession,
    spec: &ScreenSpec,
    prompt: &str,
    items: &[S],
    default: usize,
) -> Result<PromptResult<usize>> {
    session.render_with_prompt(spec, prompt)?;
    match dialoguer::Select::new()
        .with_prompt(prompt)
        .items(items)
        .default(default)
        .interact_opt()
    {
        Ok(Some(index)) => {
            render_result(
                session,
                spec,
                ScreenSeverity::Success,
                format!("Selected: {}", items[index].to_string()),
            )?;
            Ok(PromptResult::value(index))
        }
        Ok(None) => {
            render_result(
                session,
                spec,
                ScreenSeverity::Warning,
                "Cancelled by user.".to_string(),
            )?;
            Ok(PromptResult::cancelled())
        }
        Err(e) if is_cancelled_error(&e.to_string()) => {
            render_result(
                session,
                spec,
                ScreenSeverity::Warning,
                "Cancelled by user.".to_string(),
            )?;
            Ok(PromptResult::cancelled())
        }
        Err(e) => Err(e.into()),
    }
}

pub fn screen_multiselect<S: Clone + ToString>(
    session: &ScreenSession,
    spec: &ScreenSpec,
    prompt: &str,
    items: &[S],
    defaults: &[bool],
) -> Result<PromptResult<Vec<usize>>> {
    debug_assert_eq!(
        items.len(),
        defaults.len(),
        "screen_multiselect: items and defaults length mismatch"
    );
    if items.len() != defaults.len() {
        anyhow::bail!(
            "screen_multiselect: items ({}) and defaults ({}) lengths differ",
            items.len(),
            defaults.len()
        );
    }
    session.render_with_prompt(spec, prompt)?;
    match dialoguer::MultiSelect::new()
        .with_prompt(prompt)
        .items(items)
        .defaults(defaults)
        .interact_opt()
    {
        Ok(Some(indexes)) => {
            render_result(
                session,
                spec,
                ScreenSeverity::Success,
                format!("Selected {} item(s).", indexes.len()),
            )?;
            Ok(PromptResult::value(indexes))
        }
        Ok(None) => {
            render_result(
                session,
                spec,
                ScreenSeverity::Warning,
                "Cancelled by user.".to_string(),
            )?;
            Ok(PromptResult::cancelled())
        }
        Err(e) if is_cancelled_error(&e.to_string()) => {
            render_result(
                session,
                spec,
                ScreenSeverity::Warning,
                "Cancelled by user.".to_string(),
            )?;
            Ok(PromptResult::cancelled())
        }
        Err(e) => Err(e.into()),
    }
}

fn render_result(
    session: &ScreenSession,
    spec: &ScreenSpec,
    severity: ScreenSeverity,
    result_line: String,
) -> Result<()> {
    let mut spec = spec.clone().severity(severity);
    spec.body_lines.push(String::new());
    spec.body_lines.push(result_line);
    session.render(&spec)
}

fn is_cancelled_error(msg: &str) -> bool {
    let msg = msg.to_ascii_lowercase();
    msg.contains("cancel") || msg.contains("interrupt") || msg.contains("aborted")
}

pub fn format_screen(spec: &ScreenSpec, width: usize) -> String {
    let mut out = String::new();
    let step_label = match (spec.step_index, spec.step_total) {
        (Some(i), Some(t)) => format!("Step {i} of {t}"),
        _ => String::new(),
    };

    if step_label.is_empty() {
        out.push_str(&format!("◆ {}\n", spec.title));
    } else {
        let left = format!("◆ {}", spec.title);
        let spacer = width.saturating_sub(left.len() + step_label.len());
        out.push_str(&format!("{left}{}{step_label}\n", " ".repeat(spacer)));
    }
    out.push_str(&format!("{}\n", "─".repeat(width)));
    out.push('\n');

    if let Some(subtitle) = &spec.subtitle {
        out.push_str(&format!("  {subtitle}\n\n"));
    }

    for line in &spec.body_lines {
        if line.is_empty() {
            out.push('\n');
        } else {
            out.push_str(&format!("  {line}\n"));
        }
    }

    if !spec.hints.is_empty() {
        if !spec.body_lines.is_empty() {
            out.push('\n');
        }
        let marker = severity_marker(spec.severity);
        for hint in &spec.hints {
            out.push_str(&format!("  {marker} {hint}\n"));
        }
    }

    out
}

fn severity_marker(severity: ScreenSeverity) -> &'static str {
    match severity {
        ScreenSeverity::Info => "·",
        ScreenSeverity::Success => "✓",
        ScreenSeverity::Warning => "!",
        ScreenSeverity::Error => "✗",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_screen_includes_step_header() {
        let spec = ScreenSpec::new("GitHub")
            .step(1, 3)
            .body_lines(["Create token"]);
        let rendered = format_screen(&spec, 76);
        assert!(rendered.contains("◆ GitHub"));
        assert!(rendered.contains("Step 1 of 3"));
        assert!(rendered.contains("Create token"));
    }

    #[test]
    fn format_screen_renders_hints() {
        let spec = ScreenSpec::new("Railway")
            .body_lines(["Authentication failed"])
            .hints(["Check account token type"])
            .severity(ScreenSeverity::Error);
        let rendered = format_screen(&spec, 76);
        assert!(rendered.contains("✗ Check account token type"));
    }

    #[test]
    fn prompt_result_cancelled_shape() {
        let cancelled: PromptResult<String> = PromptResult::cancelled();
        assert!(cancelled.cancelled);
        assert!(cancelled.value.is_none());
    }
}
