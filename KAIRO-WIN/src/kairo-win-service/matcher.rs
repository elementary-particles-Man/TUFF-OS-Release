use aho_corasick::AhoCorasick;
use ipnet::IpNet;
use std::collections::HashSet;
use std::net::IpAddr;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Kill,
    Bypass,
    Allow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrafficDirection {
    Ingress,
    Egress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DstClass {
    Lan,
    NotLan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmp {
    Gt,
    Ge,
    Lt,
    Le,
    Eq,
}

#[derive(Debug, Clone)]
pub struct SizeCond {
    cmp: Cmp,
    bytes: usize,
}

#[derive(Debug, Clone)]
pub struct Rule {
    action: Action,
    direction: TrafficDirection,
    target_type: String,
    target_value: String,
    cond_host: Option<String>,
    cond_proto: Option<String>,
    cond_dst: Option<DstClass>,
    cond_size: Option<SizeCond>,
}

#[derive(Debug, Clone)]
pub struct RuleSet {
    rules: Vec<Rule>,
    kill_suffixes: Vec<String>,
    bypass_suffixes: Vec<String>,
    allow_suffixes: Vec<String>,
    kill_ac: Option<AhoCorasick>,
    bypass_ac: Option<AhoCorasick>,
    allow_ac: Option<AhoCorasick>,
}

#[derive(Debug, Clone)]
pub struct MatchInput<'a> {
    pub direction: TrafficDirection,
    pub dest_host: Option<&'a str>,
    pub dest_ip: Option<IpAddr>,
    pub dst_is_lan: bool,
    pub proto: Option<&'a str>,
    pub dest_port: Option<u16>,
    pub method: Option<&'a str>,
    pub payload_size: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchResult {
    pub action: Option<Action>,
    pub reason: &'static str,
}

impl RuleSet {
    pub fn empty() -> Self {
        Self {
            rules: Vec::new(),
            kill_suffixes: Vec::new(),
            bypass_suffixes: Vec::new(),
            allow_suffixes: Vec::new(),
            kill_ac: None,
            bypass_ac: None,
            allow_ac: None,
        }
    }

    pub fn from_lines(lines: &[String]) -> Result<Self, String> {
        let mut rs = Self::empty();
        for (idx, raw) in lines.iter().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 3 {
                return Err(format!("line {}: invalid rule", idx + 1));
            }

            let action = parse_action(parts[0])?;
            let (direction, tt_idx) = parse_direction(&parts, idx + 1)?;
            let target_type = parts[tt_idx].to_ascii_uppercase();
            let target_value = normalize_value(&target_type, parts[tt_idx + 1]);
            let mut rule = Rule {
                action,
                direction,
                target_type,
                target_value,
                cond_host: None,
                cond_proto: None,
                cond_dst: None,
                cond_size: None,
            };

            if action == Action::Bypass && direction == TrafficDirection::Ingress {
                return Err(format!("line {}: BYPASS INGRESS is not allowed", idx + 1));
            }

            let mut i = tt_idx + 2;
            while i < parts.len() {
                let key = parts[i].to_ascii_uppercase();
                if key == "HOST" {
                    i += 1;
                    if i >= parts.len() {
                        return Err(format!("line {}: HOST missing value", idx + 1));
                    }
                    rule.cond_host = Some(normalize_value("HOST", parts[i]));
                } else if key == "PROTO" {
                    i += 1;
                    if i >= parts.len() {
                        return Err(format!("line {}: PROTO missing value", idx + 1));
                    }
                    rule.cond_proto = Some(parts[i].to_ascii_uppercase());
                } else if key == "DST" {
                    i += 1;
                    if i >= parts.len() {
                        return Err(format!("line {}: DST missing value", idx + 1));
                    }
                    rule.cond_dst = match parts[i].to_ascii_uppercase().as_str() {
                        "LAN" => Some(DstClass::Lan),
                        "!LAN" => Some(DstClass::NotLan),
                        _ => return Err(format!("line {}: invalid DST value", idx + 1)),
                    };
                } else if key == "SIZE" {
                    if i + 1 >= parts.len() {
                        return Err(format!("line {}: SIZE missing comparator/value", idx + 1));
                    }
                    let (cmp, bytes, consumed) = if i + 2 < parts.len() {
                        if let Ok(c) = parse_cmp(parts[i + 1]) {
                            let b = parse_size(parts[i + 2])?;
                            (c, b, 2usize)
                        } else {
                            parse_cmp_and_size(parts[i + 1])?
                        }
                    } else {
                        parse_cmp_and_size(parts[i + 1])?
                    };
                    rule.cond_size = Some(SizeCond { cmp, bytes });
                    i += consumed;
                }
                i += 1;
            }

            // semantic check: SIZE requires METHOD PUT/POST target or condition
            if rule.cond_size.is_some() && direction == TrafficDirection::Egress {
                let methods = extract_methods(&rule);
                if methods.is_empty() || !methods.iter().any(|m| m == "PUT" || m == "POST") {
                    return Err(format!(
                        "line {}: SIZE requires METHOD including PUT or POST",
                        idx + 1
                    ));
                }
            }

            if rule.target_type == "HOST" && rule.target_value.starts_with("*.") {
                let suffix = rule.target_value.trim_start_matches('*').to_string();
                match rule.action {
                    Action::Kill => rs.kill_suffixes.push(suffix),
                    Action::Bypass => rs.bypass_suffixes.push(suffix),
                    Action::Allow => rs.allow_suffixes.push(suffix),
                }
            }
            rs.rules.push(rule);
        }
        rs.build_automata();
        Ok(rs)
    }

    pub fn evaluate(&self, input: &MatchInput<'_>) -> MatchResult {
        for action in [Action::Kill, Action::Bypass, Action::Allow] {
            for rule in self.rules.iter().filter(|r| r.action == action) {
                if rule_matches(self, rule, input) {
                    return MatchResult {
                        action: Some(action),
                        reason: match action {
                            Action::Kill => "kill_match",
                            Action::Bypass => "bypass_match",
                            Action::Allow => "allow_match",
                        },
                    };
                }
            }
        }
        MatchResult {
            action: None,
            reason: "default_deny",
        }
    }

    fn build_automata(&mut self) {
        self.kill_ac = build_ac(&self.kill_suffixes);
        self.bypass_ac = build_ac(&self.bypass_suffixes);
        self.allow_ac = build_ac(&self.allow_suffixes);
    }
}

fn rule_matches(rs: &RuleSet, rule: &Rule, input: &MatchInput<'_>) -> bool {
    if rule.direction != input.direction {
        return false;
    }
    if !match_target(rs, rule, input) {
        return false;
    }
    if let Some(host) = &rule.cond_host {
        if host != "*" && host != "unknown" {
            let Some(in_host) = input.dest_host else {
                return false;
            };
            if normalize_host(in_host) != *host {
                return false;
            }
        } else if host == "unknown" && input.dest_host.is_some() {
            return false;
        }
    }
    if let Some(proto) = &rule.cond_proto {
        let Some(in_proto) = input.proto else {
            return false;
        };
        if in_proto.to_ascii_uppercase() != *proto {
            return false;
        }
    }
    if let Some(dst) = rule.cond_dst {
        match dst {
            DstClass::Lan if !input.dst_is_lan => return false,
            DstClass::NotLan if input.dst_is_lan => return false,
            _ => {}
        }
    }
    if let Some(size) = &rule.cond_size {
        if !compare_size(input.payload_size, size) {
            return false;
        }
    }
    true
}

fn match_target(rs: &RuleSet, rule: &Rule, input: &MatchInput<'_>) -> bool {
    match rule.target_type.as_str() {
        "CIDR" => {
            let Some(ip) = input.dest_ip else {
                return false;
            };
            let Ok(net) = IpNet::from_str(&rule.target_value) else {
                return false;
            };
            net.contains(&ip)
        }
        "HOST" => {
            let Some(host) = input.dest_host else {
                return false;
            };
            let norm = normalize_host(host);
            if rule.target_value == "*" {
                return true;
            }
            if rule.target_value.starts_with("*.") {
                suffix_match(rs, rule.action, &norm)
            } else {
                norm == rule.target_value
            }
        }
        "PORT" => input
            .dest_port
            .map(|p| p.to_string() == rule.target_value)
            .unwrap_or(false),
        "PROTO" => input
            .proto
            .map(|p| p.to_ascii_uppercase() == rule.target_value)
            .unwrap_or(false),
        "METHOD" => input
            .method
            .map(|m| {
                let methods = rule
                    .target_value
                    .split(',')
                    .map(|x| x.trim().to_ascii_uppercase())
                    .collect::<HashSet<_>>();
                methods.contains(&m.to_ascii_uppercase())
            })
            .unwrap_or(false),
        _ => false,
    }
}

fn suffix_match(rs: &RuleSet, action: Action, host: &str) -> bool {
    let ac = match action {
        Action::Kill => &rs.kill_ac,
        Action::Bypass => &rs.bypass_ac,
        Action::Allow => &rs.allow_ac,
    };
    if let Some(matcher) = ac {
        for m in matcher.find_iter(host) {
            let suffix = &host[m.start()..m.end()];
            if host.ends_with(suffix) {
                return true;
            }
        }
    }
    false
}

fn parse_action(s: &str) -> Result<Action, String> {
    match s.to_ascii_uppercase().as_str() {
        "KILL" => Ok(Action::Kill),
        "BYPASS" => Ok(Action::Bypass),
        "ALLOW" => Ok(Action::Allow),
        _ => Err(format!("unknown action: {s}")),
    }
}

fn parse_direction(parts: &[&str], line_no: usize) -> Result<(TrafficDirection, usize), String> {
    if parts.len() < 3 {
        return Err(format!("line {}: invalid rule", line_no));
    }
    let maybe_dir = parts[1].to_ascii_uppercase();
    if maybe_dir == "INGRESS" {
        if parts.len() < 4 {
            return Err(format!("line {}: missing target after direction", line_no));
        }
        Ok((TrafficDirection::Ingress, 2))
    } else if maybe_dir == "EGRESS" {
        if parts.len() < 4 {
            return Err(format!("line {}: missing target after direction", line_no));
        }
        Ok((TrafficDirection::Egress, 2))
    } else {
        Ok((TrafficDirection::Egress, 1))
    }
}

fn normalize_host(host: &str) -> String {
    host.trim().trim_end_matches('.').to_ascii_lowercase()
}

fn normalize_value(target_type: &str, value: &str) -> String {
    match target_type {
        "HOST" => normalize_host(value),
        "PROTO" | "METHOD" => value.to_ascii_uppercase(),
        _ => value.to_string(),
    }
}

fn build_ac(patterns: &[String]) -> Option<AhoCorasick> {
    if patterns.is_empty() {
        None
    } else {
        Some(AhoCorasick::new(patterns).expect("valid AC pattern"))
    }
}

fn parse_cmp(s: &str) -> Result<Cmp, String> {
    match s {
        ">" => Ok(Cmp::Gt),
        ">=" => Ok(Cmp::Ge),
        "<" => Ok(Cmp::Lt),
        "<=" => Ok(Cmp::Le),
        "=" => Ok(Cmp::Eq),
        _ => Err(format!("invalid comparator: {s}")),
    }
}

fn parse_size(s: &str) -> Result<usize, String> {
    let up = s.to_ascii_uppercase();
    if let Some(n) = up.strip_suffix("KB") {
        return n
            .parse::<usize>()
            .map(|x| x * 1024)
            .map_err(|e| e.to_string());
    }
    if let Some(n) = up.strip_suffix("MB") {
        return n
            .parse::<usize>()
            .map(|x| x * 1024 * 1024)
            .map_err(|e| e.to_string());
    }
    if let Some(n) = up.strip_suffix('B') {
        return n.parse::<usize>().map_err(|e| e.to_string());
    }
    up.parse::<usize>().map_err(|e| e.to_string())
}

fn parse_cmp_and_size(token: &str) -> Result<(Cmp, usize, usize), String> {
    for prefix in [">=", "<=", ">", "<", "="] {
        if let Some(rest) = token.strip_prefix(prefix) {
            let cmp = parse_cmp(prefix)?;
            let bytes = parse_size(rest)?;
            return Ok((cmp, bytes, 1));
        }
    }
    Err(format!("invalid comparator+size: {}", token))
}

fn compare_size(v: usize, cond: &SizeCond) -> bool {
    match cond.cmp {
        Cmp::Gt => v > cond.bytes,
        Cmp::Ge => v >= cond.bytes,
        Cmp::Lt => v < cond.bytes,
        Cmp::Le => v <= cond.bytes,
        Cmp::Eq => v == cond.bytes,
    }
}

fn extract_methods(rule: &Rule) -> HashSet<String> {
    if rule.target_type == "METHOD" {
        return rule
            .target_value
            .split(',')
            .map(|x| x.trim().to_ascii_uppercase())
            .collect();
    }
    HashSet::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;

    #[test]
    fn priority_kill_over_allow() {
        let lines = vec![
            "ALLOW HOST api.openai.com".to_string(),
            "KILL HOST api.openai.com".to_string(),
        ];
        let rs = RuleSet::from_lines(&lines).unwrap();
        let out = rs.evaluate(&MatchInput {
            direction: TrafficDirection::Egress,
            dest_host: Some("api.openai.com"),
            dest_ip: None,
            dst_is_lan: false,
            proto: Some("HTTPS"),
            dest_port: Some(443),
            method: Some("POST"),
            payload_size: 20,
        });
        assert_eq!(out.action, Some(Action::Kill));
    }

    #[test]
    fn dst_not_lan_rule() {
        let lines = vec!["KILL PROTO SMB DST !LAN".to_string()];
        let rs = RuleSet::from_lines(&lines).unwrap();
        let out = rs.evaluate(&MatchInput {
            direction: TrafficDirection::Egress,
            dest_host: None,
            dest_ip: Some(IpAddr::from_str("8.8.8.8").unwrap()),
            dst_is_lan: false,
            proto: Some("SMB"),
            dest_port: Some(445),
            method: None,
            payload_size: 0,
        });
        assert_eq!(out.action, Some(Action::Kill));
    }

    #[test]
    fn method_size_rule() {
        let lines = vec!["KILL METHOD PUT,POST SIZE >100KB HOST *".to_string()];
        let rs = RuleSet::from_lines(&lines).unwrap();
        let out = rs.evaluate(&MatchInput {
            direction: TrafficDirection::Egress,
            dest_host: Some("unknown.example"),
            dest_ip: None,
            dst_is_lan: false,
            proto: Some("HTTPS"),
            dest_port: Some(443),
            method: Some("POST"),
            payload_size: 150 * 1024,
        });
        assert_eq!(out.action, Some(Action::Kill));
    }

    #[test]
    fn ingress_rule_respected() {
        let lines = vec!["KILL INGRESS PORT 22 HOST *".to_string()];
        let rs = RuleSet::from_lines(&lines).unwrap();
        let egress = rs.evaluate(&MatchInput {
            direction: TrafficDirection::Egress,
            dest_host: Some("x"),
            dest_ip: None,
            dst_is_lan: false,
            proto: Some("TCP"),
            dest_port: Some(22),
            method: None,
            payload_size: 0,
        });
        assert_eq!(egress.action, None);
        let ingress = rs.evaluate(&MatchInput {
            direction: TrafficDirection::Ingress,
            dest_host: Some("x"),
            dest_ip: None,
            dst_is_lan: false,
            proto: Some("TCP"),
            dest_port: Some(22),
            method: None,
            payload_size: 0,
        });
        assert_eq!(ingress.action, Some(Action::Kill));
    }
}
