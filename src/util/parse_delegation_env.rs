// src/util/parse_delegation_env.rs

use std::collections::HashMap;

pub fn parse_delegation_env(contents: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }
        if trimmed.starts_with('#') { continue; }

        let Some(eq) = trimmed.find('=') else { continue; };
        let (k, vraw) = trimmed.split_at(eq);
        let key = k.trim().to_string();

        let mut val = vraw[1..].trim().to_string();

        // Optional surrounding quotes "..." or '...'
        if (val.starts_with('"') && val.ends_with('"') && val.len() >= 2)
            || (val.starts_with('\'') && val.ends_with('\'') && val.len() >= 2)
        {
            val = val[1..val.len()-1].to_string();
        }

        // Normalize keys to exactly what the input screen expects
        // (we keep original casing as provided in the spec)
        out.insert(key, val);
    }

    out
}

