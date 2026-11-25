use crate::plan::Method;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Budgets {
    pub seconds: HashMap<Method, u64>,
}

impl Default for Budgets {
    fn default() -> Self {
        let mut seconds = HashMap::new();
        seconds.insert(Method::A, 60);
        seconds.insert(Method::E, 45);
        seconds.insert(Method::D, 45);
        seconds.insert(Method::C, 75);
        seconds.insert(Method::P, 90);
        seconds.insert(Method::X, 45);
        Self { seconds }
    }
}

impl Budgets {
    pub fn get(&self, method: Method) -> u64 {
        self.seconds.get(&method).copied().unwrap_or(45)
    }

    /// Overlay only the keys the file actually sets. Unknown names are ignored
    /// so a typo cannot silently change another rung.
    pub fn apply_file(&mut self, raw: &HashMap<String, u64>) {
        for (k, v) in raw {
            if let Some(m) = Method::parse(k) {
                self.seconds.insert(m, *v);
            }
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct BudgetFile {
    #[serde(default)]
    pub budgets: HashMap<String, u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_measured_ladder() {
        let b = Budgets::default();
        assert_eq!(b.get(Method::A), 60);
        assert_eq!(b.get(Method::C), 75);
        assert_eq!(b.get(Method::P), 90);
        assert_eq!(b.get(Method::X), 45);
    }

    #[test]
    fn file_overrides_only_named_rungs() {
        let mut b = Budgets::default();
        let mut raw = HashMap::new();
        raw.insert("A".into(), 12);
        raw.insert("nope".into(), 1);
        b.apply_file(&raw);
        assert_eq!(b.get(Method::A), 12);
        assert_eq!(b.get(Method::E), 45);
    }
}
