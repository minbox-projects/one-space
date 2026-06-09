use crate::subagents::{parse_frontmatter_value, MODELS};

pub(in crate::subagents) fn normalized_model(value: &str) -> Option<String> {
    let v = value.trim().to_lowercase();
    if MODELS.contains(&v.as_str()) {
        Some(v)
    } else {
        None
    }
}

pub(in crate::subagents) fn normalize_models(models: &[String]) -> Vec<String> {
    let mut out = vec![];
    for raw in models {
        if let Some(m) = normalized_model(raw) {
            if !out.contains(&m) {
                out.push(m);
            }
        }
    }
    out
}

pub(in crate::subagents) fn all_models_vec() -> Vec<String> {
    MODELS.iter().map(|v| v.to_string()).collect()
}

pub(in crate::subagents) fn resolve_effective_models(
    declared_models: &[String],
    source_allowed_models: &[String],
) -> Vec<String> {
    let mut declared = normalize_models(declared_models);
    if declared.is_empty() {
        declared = all_models_vec();
    }
    let allowed = normalize_models(source_allowed_models);
    if allowed.is_empty() {
        return declared;
    }
    declared
        .into_iter()
        .filter(|model| allowed.contains(model))
        .collect::<Vec<_>>()
}

pub(in crate::subagents) fn parse_models(text: &str, source_default: &[String]) -> Vec<String> {
    let mut out = vec![];
    for line in text.lines() {
        let lower = line.trim().to_lowercase();
        if lower.starts_with("models:") {
            let body = line.split_once(':').map(|(_, v)| v).unwrap_or("").trim();
            let body = body.trim_matches('[').trim_matches(']');
            for item in body.split(',') {
                let m = item
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_lowercase();
                if MODELS.contains(&m.as_str()) && !out.contains(&m) {
                    out.push(m);
                }
            }
            if !out.is_empty() {
                return out;
            }
        }
    }
    for v in normalize_models(source_default) {
        if !out.contains(&v) {
            out.push(v);
        }
    }
    if out.is_empty() {
        all_models_vec()
    } else {
        out
    }
}

pub(in crate::subagents) fn normalize_subagent_markdown_for_parse(md: &str) -> String {
    let no_bom = md.strip_prefix('\u{feff}').unwrap_or(md);
    no_bom.replace("\r\n", "\n").replace('\r', "\n")
}

pub(in crate::subagents) fn split_frontmatter_block(md: &str) -> (Option<&str>, &str) {
    let trimmed = md.trim_start_matches(|c: char| c.is_whitespace());
    if !trimmed.starts_with("---\n") {
        return (None, md);
    }

    let body = &trimmed[4..];
    let mut cursor = 0usize;
    for segment in body.split_inclusive('\n') {
        let line = segment.trim_end_matches('\n').trim_end_matches('\r');
        if line.trim() == "---" {
            let frontmatter = if cursor > 0 {
                &body[..cursor.saturating_sub(1)]
            } else {
                ""
            };
            let content = &body[cursor + segment.len()..];
            return (Some(frontmatter), content);
        }
        cursor += segment.len();
    }

    if body.trim() == "---" {
        return (Some(""), "");
    }

    (None, md)
}

pub(in crate::subagents) fn parse_title_and_description(
    content: &str,
) -> (Option<String>, Option<String>) {
    let mut title = None;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            let candidate = trimmed.trim_start_matches('#').trim().to_string();
            if !candidate.is_empty() {
                title = Some(candidate);
                break;
            }
        }
    }

    let mut desc = String::new();
    let mut in_para = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if in_para {
                break;
            }
            continue;
        }
        if trimmed.starts_with('#') {
            continue;
        }
        in_para = true;
        if !desc.is_empty() {
            desc.push(' ');
        }
        desc.push_str(trimmed);
    }

    let description = if desc.is_empty() { None } else { Some(desc) };
    (title, description)
}

pub(in crate::subagents) fn parse_subagent_md(
    md: &str,
    source_default_models: &[String],
) -> (String, String, Vec<String>) {
    let normalized = normalize_subagent_markdown_for_parse(md);
    let (frontmatter, mut content) = split_frontmatter_block(&normalized);
    let mut name_from_frontmatter = None;
    let mut description_from_frontmatter = None;
    let mut models = parse_models(&normalized, source_default_models);
    if let Some(front) = frontmatter {
        models = parse_models(front, source_default_models);
        name_from_frontmatter = parse_frontmatter_value(front, "name");
        description_from_frontmatter = parse_frontmatter_value(front, "description");
        content = content.trim_start_matches('\n');
    }

    let (name, desc) = if frontmatter.is_some() {
        (
            name_from_frontmatter.unwrap_or_else(|| "Unnamed Subagent".to_string()),
            description_from_frontmatter.unwrap_or_else(|| "No description".to_string()),
        )
    } else {
        let (title_from_content, paragraph_from_content) = parse_title_and_description(content);
        (
            title_from_content.unwrap_or_else(|| "Unnamed Subagent".to_string()),
            paragraph_from_content.unwrap_or_else(|| "No description".to_string()),
        )
    };
    (name, desc, models)
}

pub(in crate::subagents) fn parse_frontmatter_list_value(
    frontmatter: &str,
    key: &str,
) -> Vec<String> {
    let mut out = vec![];
    let mut lines = frontmatter.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((k, v)) = trimmed.split_once(':') else {
            continue;
        };
        if !k.trim().eq_ignore_ascii_case(key) {
            continue;
        }

        let value = v.trim();
        if !value.is_empty() {
            let normalized = value.trim_matches('[').trim_matches(']');
            for item in normalized.split(',') {
                let token = item
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .trim()
                    .to_string();
                if !token.is_empty() && !out.contains(&token) {
                    out.push(token);
                }
            }
            return out;
        }

        while let Some(next) = lines.peek() {
            let next_trimmed = next.trim();
            if next_trimmed.is_empty() {
                lines.next();
                continue;
            }
            if let Some(rest) = next_trimmed.strip_prefix('-') {
                let token = rest
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .trim()
                    .to_string();
                if !token.is_empty() && !out.contains(&token) {
                    out.push(token);
                }
                lines.next();
                continue;
            }
            break;
        }
        return out;
    }
    out
}

pub(in crate::subagents) fn parse_subagent_frontmatter_meta(
    md: &str,
) -> (Option<String>, Vec<String>) {
    let normalized = normalize_subagent_markdown_for_parse(md);
    let (frontmatter, _) = split_frontmatter_block(&normalized);
    let Some(frontmatter) = frontmatter else {
        return (None, vec![]);
    };
    let model = parse_frontmatter_value(frontmatter, "model");
    let tools = parse_frontmatter_list_value(frontmatter, "tools");
    (model, tools)
}

pub(in crate::subagents) fn validate_frontmatter_name_as_dir(
    value: &str,
) -> Result<String, String> {
    let name = value.trim();
    if name.is_empty() || name == "." || name == ".." {
        return Err("subagents/invalid_frontmatter_name".to_string());
    }
    if name.contains('/') || name.contains('\\') {
        return Err("subagents/invalid_frontmatter_name".to_string());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err("subagents/invalid_frontmatter_name".to_string());
    }
    Ok(name.to_string())
}

pub(in crate::subagents) fn parse_required_subagent_dir_name(md: &str) -> Result<String, String> {
    let normalized = normalize_subagent_markdown_for_parse(md);
    let (frontmatter, _) = split_frontmatter_block(&normalized);
    let frontmatter = frontmatter.ok_or("subagents/invalid_frontmatter_name".to_string())?;
    let raw_name = parse_frontmatter_value(frontmatter, "name")
        .ok_or("subagents/invalid_frontmatter_name".to_string())?;
    validate_frontmatter_name_as_dir(&raw_name)
}

pub(in crate::subagents) fn diagnose_frontmatter_name_error(md: &str) -> Option<String> {
    let normalized = normalize_subagent_markdown_for_parse(md);
    let (frontmatter, _) = split_frontmatter_block(&normalized);
    let frontmatter = frontmatter?;
    let Some(raw_name) = parse_frontmatter_value(frontmatter, "name") else {
        return Some("missing_name".to_string());
    };
    if validate_frontmatter_name_as_dir(&raw_name).is_ok() {
        None
    } else {
        Some("invalid_name".to_string())
    }
}
