use std::fs;
use std::path::Path;

use crate::matcher::RuleSet;

pub const DEFAULT_ACL_PATH: &str = "KAIRO-ACL.txt";

#[derive(Clone)]
pub struct LoadedAcl {
    pub rules: RuleSet,
    pub normalized_lines: Vec<String>,
    pub allow_count: usize,
}

pub fn load_acl_from_path(path: &Path) -> Result<LoadedAcl, String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("read {:?}: {}", path, e))?;
    load_acl_from_str(&raw)
}

pub fn load_acl_from_str(input: &str) -> Result<LoadedAcl, String> {
    let mut normalized = Vec::new();
    let mut allow_count = 0usize;

    for (idx, raw_line) in input.lines().enumerate() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            return Err(format!("line {}: requires at least 3 tokens", idx + 1));
        }
        let action = parts[0].to_ascii_uppercase();
        let (direction, tt_idx) = if parts[1].eq_ignore_ascii_case("INGRESS")
            || parts[1].eq_ignore_ascii_case("EGRESS")
        {
            if parts.len() < 4 {
                return Err(format!(
                    "line {}: missing target type after direction",
                    idx + 1
                ));
            }
            (Some(parts[1].to_ascii_uppercase()), 2usize)
        } else {
            (None, 1usize)
        };
        let target = parts[tt_idx].to_ascii_uppercase();
        if action != "ALLOW" && action != "KILL" && action != "BYPASS" {
            return Err(format!("line {}: invalid action {}", idx + 1, parts[0]));
        }
        if action == "ALLOW" {
            allow_count += 1;
        }
        match target.as_str() {
            "HOST" | "CIDR" | "PORT" | "PROTO" | "METHOD" => {}
            _ => {
                return Err(format!(
                    "line {}: invalid target type {}",
                    idx + 1,
                    parts[1]
                ))
            }
        }

        // strict-yet-pragmatic checks aligned with EBNF draft
        if target == "CIDR" && !parts[tt_idx + 1].contains('/') {
            return Err(format!("line {}: CIDR missing prefix", idx + 1));
        }
        if target == "PORT" && parts[tt_idx + 1].parse::<u16>().is_err() {
            return Err(format!(
                "line {}: invalid PORT {}",
                idx + 1,
                parts[tt_idx + 1]
            ));
        }
        if target == "METHOD" && !parts[tt_idx + 1].contains(',') && parts[tt_idx + 1].len() < 3 {
            return Err(format!("line {}: invalid METHOD field", idx + 1));
        }

        let mut rebuilt = Vec::with_capacity(parts.len());
        rebuilt.push(action);
        if let Some(d) = direction {
            rebuilt.push(d);
        }
        rebuilt.push(target);
        rebuilt.push(normalize_value(parts[tt_idx], parts[tt_idx + 1]));
        for token in parts.iter().skip(tt_idx + 2) {
            rebuilt.push(normalize_token(token));
        }
        normalized.push(rebuilt.join(" "));
    }

    if allow_count == 0 {
        return Err("ACL must contain at least one ALLOW rule".to_string());
    }

    let rules = RuleSet::from_lines(&normalized)?;
    Ok(LoadedAcl {
        rules,
        normalized_lines: normalized,
        allow_count,
    })
}

fn normalize_value(target_type: &str, value: &str) -> String {
    match target_type.to_ascii_uppercase().as_str() {
        "HOST" => value.trim().trim_end_matches('.').to_ascii_lowercase(),
        "PROTO" | "METHOD" => value.to_ascii_uppercase(),
        _ => value.to_string(),
    }
}

fn normalize_token(t: &str) -> String {
    if t.eq_ignore_ascii_case("HOST")
        || t.eq_ignore_ascii_case("PROTO")
        || t.eq_ignore_ascii_case("DST")
        || t.eq_ignore_ascii_case("SIZE")
    {
        t.to_ascii_uppercase()
    } else {
        t.trim().trim_end_matches('.').to_ascii_lowercase()
    }
}
