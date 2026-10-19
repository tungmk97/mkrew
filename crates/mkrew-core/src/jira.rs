//! Jira project keys of a workspace (SPEC §20I, ADR 0062).

/// Trims, upper-cases, drops empty entries and duplicates (first one wins), then checks each
/// key is a Jira project key (`^[A-Z][A-Z0-9_]+$`). The error names the first bad key.
pub fn normalize_keys(keys: Vec<String>) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for key in keys {
        let key = key.trim().to_ascii_uppercase();
        if key.is_empty() || out.contains(&key) {
            continue;
        }
        let mut chars = key.chars();
        let first_ok = chars.next().is_some_and(|c| c.is_ascii_uppercase());
        let rest_ok = key.len() >= 2
            && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
        if !(first_ok && rest_ok) {
            return Err(format!(
                "{key:?} is not a Jira project key (letters, digits and _, starting with a letter)"
            ));
        }
        out.push(key);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn keys_are_trimmed_upper_cased_and_deduplicated_in_order() {
        assert_eq!(
            normalize_keys(keys(&[" proj ", "Bill", "PROJ", "", "  "])).unwrap(),
            keys(&["PROJ", "BILL"])
        );
        assert_eq!(normalize_keys(Vec::new()).unwrap(), Vec::<String>::new());
        assert_eq!(normalize_keys(keys(&["A1_B"])).unwrap(), keys(&["A1_B"]));
    }

    #[test]
    fn non_keys_are_refused_by_name() {
        for bad in ["PR-1", "1AB", "P", "PRÓJ", "PR OJ"] {
            let error = normalize_keys(keys(&["OK", bad])).unwrap_err();
            assert!(error.contains(&bad.to_ascii_uppercase()), "{bad}: {error}");
        }
    }
}
