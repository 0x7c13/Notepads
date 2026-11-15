use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug, Default, Clone)]
pub(super) struct StateRenderDetails {
    pub(super) root_calls: u32,
    pub(super) clusters: std::time::Duration,
    pub(super) edge_paths: std::time::Duration,
    pub(super) edge_labels: std::time::Duration,
    pub(super) leaf_nodes: std::time::Duration,
    pub(super) leaf_nodes_style_parse: std::time::Duration,
    pub(super) leaf_nodes_roughjs: std::time::Duration,
    pub(super) leaf_roughjs_calls: u32,
    pub(super) leaf_roughjs_unique: std::collections::HashSet<StateRoughCacheKey>,
    pub(super) leaf_nodes_measure: std::time::Duration,
    pub(super) leaf_nodes_label_html: std::time::Duration,
    pub(super) leaf_nodes_emit: std::time::Duration,
    pub(super) nested_roots: std::time::Duration,
    pub(super) self_loop_placeholders: std::time::Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct StateRoughCacheKey {
    pub(super) tag: u8,
    pub(super) a: u64,
    pub(super) b: u64,
    pub(super) seed: roughr::core::RoughJsSeed,
}

#[derive(Clone)]
enum StateRoughGeometry {
    Circle(Rc<String>),
    Paths(Rc<String>, Rc<String>),
}

#[derive(Default)]
pub(super) struct StateRoughCache {
    entries: RefCell<FxHashMap<StateRoughCacheKey, StateRoughGeometry>>,
}

impl StateRoughCache {
    pub(super) fn get_or_build_circle(
        &self,
        key: StateRoughCacheKey,
        build: impl FnOnce() -> String,
    ) -> Rc<String> {
        // A fallback `Math.random()` stream is ordered across shapes, so cache hits would skip
        // stream consumption and change subsequent output.
        if key.seed.may_use_math_random() {
            return Rc::new(build());
        }

        {
            let entries = self.entries.borrow();
            match entries.get(&key) {
                Some(StateRoughGeometry::Circle(value)) => return Rc::clone(value),
                Some(StateRoughGeometry::Paths(..)) => {
                    panic!("State Rough cache key reused for circle and path geometry")
                }
                None => {}
            }
        }

        let value = Rc::new(build());
        let previous = self
            .entries
            .borrow_mut()
            .insert(key, StateRoughGeometry::Circle(Rc::clone(&value)));
        debug_assert!(previous.is_none(), "State Rough circle inserted twice");
        value
    }

    pub(super) fn get_or_build_paths(
        &self,
        key: StateRoughCacheKey,
        build: impl FnOnce() -> (String, String),
    ) -> (Rc<String>, Rc<String>) {
        if key.seed.may_use_math_random() {
            let (fill, stroke) = build();
            return (Rc::new(fill), Rc::new(stroke));
        }

        {
            let entries = self.entries.borrow();
            match entries.get(&key) {
                Some(StateRoughGeometry::Paths(fill, stroke)) => {
                    return (Rc::clone(fill), Rc::clone(stroke));
                }
                Some(StateRoughGeometry::Circle(..)) => {
                    panic!("State Rough cache key reused for path and circle geometry")
                }
                None => {}
            }
        }

        let (fill, stroke) = build();
        let value = (Rc::new(fill), Rc::new(stroke));
        let previous = self.entries.borrow_mut().insert(
            key,
            StateRoughGeometry::Paths(Rc::clone(&value.0), Rc::clone(&value.1)),
        );
        debug_assert!(previous.is_none(), "State Rough paths inserted twice");
        value
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.borrow().len()
    }
}

#[inline]
pub(super) fn detail_guard<'a>(
    timing: super::timing::RenderTiming,
    dst: &'a mut std::time::Duration,
) -> Option<super::timing::TimingGuard<'a>> {
    timing.section(dst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn key_with_seed(tag: u8, seed: f64) -> StateRoughCacheKey {
        StateRoughCacheKey {
            tag,
            a: 10,
            b: 20,
            seed: roughr::core::RoughJsSeed::new(seed),
        }
    }

    fn key(tag: u8) -> StateRoughCacheKey {
        key_with_seed(tag, 7.0)
    }

    #[test]
    fn seeded_geometry_is_reused_within_one_operation() {
        let cache = StateRoughCache::default();
        let circle_builds = Cell::new(0usize);
        let first = cache.get_or_build_circle(key(1), || {
            circle_builds.set(circle_builds.get() + 1);
            "circle".to_string()
        });
        let second = cache.get_or_build_circle(key(1), || {
            circle_builds.set(circle_builds.get() + 1);
            "unreachable".to_string()
        });

        assert!(Rc::ptr_eq(&first, &second));
        assert_eq!(circle_builds.get(), 1);

        let path_builds = Cell::new(0usize);
        let first = cache.get_or_build_paths(key(2), || {
            path_builds.set(path_builds.get() + 1);
            ("fill".to_string(), "stroke".to_string())
        });
        let second = cache.get_or_build_paths(key(2), || {
            path_builds.set(path_builds.get() + 1);
            ("unreachable".to_string(), "unreachable".to_string())
        });

        assert!(Rc::ptr_eq(&first.0, &second.0));
        assert!(Rc::ptr_eq(&first.1, &second.1));
        assert_eq!(path_builds.get(), 1);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn operation_caches_do_not_share_geometry() {
        let first_cache = StateRoughCache::default();
        let second_cache = StateRoughCache::default();
        let first = first_cache.get_or_build_circle(key(1), || "first".to_string());
        let second = second_cache.get_or_build_circle(key(1), || "second".to_string());

        assert_eq!(first.as_str(), "first");
        assert_eq!(second.as_str(), "second");
        assert!(!Rc::ptr_eq(&first, &second));
        assert_eq!(first_cache.len(), 1);
        assert_eq!(second_cache.len(), 1);
    }

    #[test]
    fn fallback_geometry_bypasses_operation_cache() {
        let cache = StateRoughCache::default();
        let circle_builds = Cell::new(0usize);
        let fallback_key = key_with_seed(1, 4_294_967_296.0);
        let first = cache.get_or_build_circle(fallback_key, || {
            circle_builds.set(circle_builds.get() + 1);
            "first".to_string()
        });
        let second = cache.get_or_build_circle(fallback_key, || {
            circle_builds.set(circle_builds.get() + 1);
            "second".to_string()
        });

        assert_eq!(first.as_str(), "first");
        assert_eq!(second.as_str(), "second");
        assert!(!Rc::ptr_eq(&first, &second));
        assert_eq!(circle_builds.get(), 2);

        let path_builds = Cell::new(0usize);
        let fallback_key = key_with_seed(2, -1.0);
        let first = cache.get_or_build_paths(fallback_key, || {
            path_builds.set(path_builds.get() + 1);
            ("first-fill".to_string(), "first-stroke".to_string())
        });
        let second = cache.get_or_build_paths(fallback_key, || {
            path_builds.set(path_builds.get() + 1);
            ("second-fill".to_string(), "second-stroke".to_string())
        });

        assert!(!Rc::ptr_eq(&first.0, &second.0));
        assert!(!Rc::ptr_eq(&first.1, &second.1));
        assert_eq!(path_builds.get(), 2);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn operation_cache_releases_geometry_after_error_and_unwind() {
        fn fail_after_populating() -> (Result<(), &'static str>, std::rc::Weak<String>) {
            let cache = StateRoughCache::default();
            let value = cache.get_or_build_circle(key(1), || "error".to_string());
            let weak = Rc::downgrade(&value);
            (Err("expected failure"), weak)
        }

        let (result, weak) = fail_after_populating();
        assert_eq!(result, Err("expected failure"));
        assert!(weak.upgrade().is_none());

        let mut unwind_weak = None;
        let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let cache = StateRoughCache::default();
            let value = cache.get_or_build_circle(key(1), || "unwind".to_string());
            unwind_weak = Some(Rc::downgrade(&value));
            panic!("expected unwind");
        }));
        assert!(unwind.is_err());
        assert!(
            unwind_weak
                .expect("unwind should capture a geometry witness")
                .upgrade()
                .is_none()
        );
    }

    #[test]
    fn concurrent_operations_keep_independent_caches() {
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let handles: Vec<_> = (0..2)
            .map(|worker| {
                let barrier = std::sync::Arc::clone(&barrier);
                std::thread::spawn(move || {
                    let cache = StateRoughCache::default();
                    barrier.wait();
                    let value = cache.get_or_build_circle(key(1), || format!("worker-{worker}"));
                    let reused = cache.get_or_build_circle(key(1), || "unreachable".to_string());
                    assert!(Rc::ptr_eq(&value, &reused));
                    value.as_str().to_string()
                })
            })
            .collect();
        let values: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().expect("State Rough worker should finish"))
            .collect();

        assert_eq!(values, ["worker-0", "worker-1"]);
    }
}
