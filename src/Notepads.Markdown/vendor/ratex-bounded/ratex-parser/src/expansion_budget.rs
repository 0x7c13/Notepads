// Notepads Markdown bounded modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
//! Cumulative allocation accounting for controlled macro/mhchem builders.
//! Moving owned data does not consume this budget; it is not an RSS/CPU limit.
use ratex_types::{
    execution::Phase,
    storage::{LocalShared, StorageError, StorageLedger},
};
use std::fmt::{self, Write};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Cost {
    pub tokens: usize,
    pub bytes: usize,
    pub nodes: usize,
}

impl Cost {
    pub(crate) fn checked_add(self, rhs: Self) -> Option<Self> {
        Some(Self {
            tokens: self.tokens.checked_add(rhs.tokens)?,
            bytes: self.bytes.checked_add(rhs.bytes)?,
            nodes: self.nodes.checked_add(rhs.nodes)?,
        })
    }
    pub(crate) fn checked_mul(self, n: usize) -> Option<Self> {
        Some(Self {
            tokens: self.tokens.checked_mul(n)?,
            bytes: self.bytes.checked_mul(n)?,
            nodes: self.nodes.checked_mul(n)?,
        })
    }
    pub(crate) fn token(text: &str) -> Self {
        Self {
            tokens: 1,
            bytes: text.len(),
            nodes: 0,
        }
    }
}

const DEFAULT_LIMITS: Cost = Cost {
    tokens: 100_000,
    bytes: 8 * 1024 * 1024,
    nodes: 100_000,
};

#[derive(Clone, Debug)]
pub(crate) struct ExpansionBudget(BudgetOwner);
#[derive(Clone, Debug)]
enum BudgetOwner {
    Legacy(Arc<Mutex<State>>),
    Bounded(LocalShared<Mutex<State>>),
}
impl std::ops::Deref for BudgetOwner {
    type Target = Mutex<State>;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Legacy(value) => value,
            Self::Bounded(value) => value,
        }
    }
}
#[derive(Debug)]
struct State {
    used: Cost,
    limits: Cost,
    calls: usize,
}

#[derive(Debug)]
pub(crate) struct ResourceExceeded {
    site: &'static str,
    used: Cost,
    requested: Option<Cost>,
    limits: Cost,
    calls: usize,
}
impl fmt::Display for ResourceExceeded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Macro expansion resource limit exceeded at {}: used={:?}, requested={:?}, limits={:?}, macro_calls={}",
            self.site, self.used, self.requested, self.limits, self.calls
        )
    }
}
impl std::error::Error for ResourceExceeded {}
impl From<ResourceExceeded> for crate::ParseError {
    fn from(e: ResourceExceeded) -> Self {
        Self::msg(e.to_string())
    }
}
impl From<ResourceExceeded> for crate::mhchem::MhchemError {
    fn from(e: ResourceExceeded) -> Self {
        Self::msg(e.to_string())
    }
}

impl Default for ExpansionBudget {
    fn default() -> Self {
        Self::new(DEFAULT_LIMITS)
    }
}
impl ExpansionBudget {
    pub(crate) fn new(limits: Cost) -> Self {
        Self(BudgetOwner::Legacy(Arc::new(Mutex::new(State {
            used: Cost::default(),
            limits,
            calls: 0,
        }))))
    }
    pub(crate) fn new_bounded(storage: &StorageLedger<'_>) -> Result<Self, StorageError> {
        Ok(Self(BudgetOwner::Bounded(LocalShared::try_new(
            storage,
            Phase::Macro,
            Mutex::new(State {
                used: Cost::default(),
                limits: Cost {
                    tokens: 4096,
                    bytes: 65536,
                    nodes: 8192,
                },
                calls: 0,
            }),
        )?)))
    }
    pub(crate) fn charge(
        &self,
        cost: Option<Cost>,
        site: &'static str,
    ) -> Result<(), ResourceExceeded> {
        let mut s = self.0.lock().unwrap();
        if let Some(next) = cost.and_then(|c| s.used.checked_add(c)) {
            if next.tokens <= s.limits.tokens
                && next.bytes <= s.limits.bytes
                && next.nodes <= s.limits.nodes
            {
                s.used = next;
                return Ok(());
            }
        }
        Err(ResourceExceeded {
            site,
            used: s.used,
            requested: cost,
            limits: s.limits,
            calls: s.calls,
        })
    }
    pub(crate) fn calls(&self, calls: usize) {
        self.0.lock().unwrap().calls = calls;
    }
    #[cfg(test)]
    pub(crate) fn used(&self) -> Cost {
        self.0.lock().unwrap().used
    }

    pub(crate) fn text(&self, text: &str) -> Result<String, ResourceExceeded> {
        self.charge(
            Some(Cost {
                bytes: text.len(),
                ..Cost::default()
            }),
            "text copy",
        )?;
        Ok(text.to_owned())
    }
    pub(crate) fn append(&self, out: &mut String, text: &str) -> Result<(), ResourceExceeded> {
        self.charge(
            Some(Cost {
                bytes: text.len(),
                ..Cost::default()
            }),
            "text append",
        )?;
        out.push_str(text);
        Ok(())
    }
    // Count formatting without allocating, then reserve before writing any output.
    // Only internal, deterministic Display implementations (str/numbers) are used.
    pub(crate) fn format(&self, args: fmt::Arguments<'_>) -> Result<String, ResourceExceeded> {
        struct Counter(Option<usize>);
        impl Write for Counter {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                self.0 = self.0.and_then(|n| n.checked_add(s.len()));
                self.0.map(|_| ()).ok_or(fmt::Error)
            }
        }
        let mut count = Counter(Some(0));
        let _ = count.write_fmt(args);
        self.charge(
            count.0.map(|bytes| Cost {
                bytes,
                ..Cost::default()
            }),
            "formatted text",
        )?;
        let mut out = String::with_capacity(count.0.unwrap());
        out.write_fmt(args)
            .expect("internal formatting is infallible");
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atomic_boundaries_and_overflow() {
        let limits = Cost {
            tokens: 2,
            bytes: 3,
            nodes: 1,
        };
        let b = ExpansionBudget::new(limits);
        b.charge(Some(limits), "boundary").unwrap();
        for cost in [
            Some(Cost::token("x")),
            None,
            Some(Cost {
                nodes: 1,
                ..Cost::default()
            }),
        ] {
            assert!(b.charge(cost, "reject").is_err());
            assert_eq!(b.used(), limits);
        }
        assert!(limits.checked_mul(usize::MAX).is_none());
        assert!(
            limits
                .checked_add(Cost {
                    bytes: usize::MAX,
                    ..Cost::default()
                })
                .is_none()
        );
    }
    #[test]
    fn shared_utf8_and_formatting() {
        let b = ExpansionBudget::new(Cost {
            bytes: 5,
            ..Cost::default()
        });
        assert_eq!(b.text("é").unwrap(), "é");
        assert_eq!(
            b.clone().format(format_args!("{}{}", '中', "")).unwrap(),
            "中"
        );
        assert_eq!(b.used().bytes, 5);
        assert!(b.text("x").is_err());
        assert_eq!(b.used().bytes, 5);
    }
}
