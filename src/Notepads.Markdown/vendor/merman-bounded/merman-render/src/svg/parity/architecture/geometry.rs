use super::super::Bounds;
use crate::architecture_metrics::architecture_svg_group_bbox_padding_px;

pub(super) fn is_arch_dir_x(dir: char) -> bool {
    matches!(dir, 'L' | 'R')
}

pub(super) fn is_arch_dir_y(dir: char) -> bool {
    matches!(dir, 'T' | 'B')
}

pub(super) fn arrow_shift(dir: char, orig: f64, arrow_size: f64) -> f64 {
    // Mermaid@11.12.2 `ArchitectureDirectionArrowShift`.
    match dir {
        'L' | 'T' => orig - arrow_size + 2.0,
        'R' | 'B' => orig - 2.0,
        _ => orig,
    }
}

pub(super) fn extend_bounds(bounds: &mut Option<Bounds>, other: Bounds) {
    let b = bounds.get_or_insert(other.clone());
    b.min_x = b.min_x.min(other.min_x);
    b.min_y = b.min_y.min(other.min_y);
    b.max_x = b.max_x.max(other.max_x);
    b.max_y = b.max_y.max(other.max_y);
}

pub(super) fn bounds_from_rect(x: f64, y: f64, w: f64, h: f64) -> Bounds {
    Bounds {
        min_x: x,
        min_y: y,
        max_x: x + w,
        max_y: y + h,
    }
}

#[derive(Clone, Copy)]
pub(super) struct GroupRect<'a> {
    pub(super) id: &'a str,
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) w: f64,
    pub(super) h: f64,
    pub(super) icon: Option<&'a str>,
    pub(super) title: Option<&'a str>,
}

pub(super) struct GroupRectComputer<'a> {
    icon_size_px: f64,
    padding_px: f64,
    services_in_group: &'a rustc_hash::FxHashMap<&'a str, Vec<&'a str>>,
    junctions_in_group: &'a rustc_hash::FxHashMap<&'a str, Vec<&'a str>>,
    child_groups: &'a rustc_hash::FxHashMap<&'a str, Vec<&'a str>>,
    service_bounds: &'a rustc_hash::FxHashMap<&'a str, Bounds>,
    junction_bounds: &'a rustc_hash::FxHashMap<&'a str, Bounds>,
    group_rects: rustc_hash::FxHashMap<&'a str, Bounds>,
    visiting: rustc_hash::FxHashSet<&'a str>,
}

impl<'a> GroupRectComputer<'a> {
    pub(super) fn new(
        icon_size_px: f64,
        padding_px: f64,
        services_in_group: &'a rustc_hash::FxHashMap<&'a str, Vec<&'a str>>,
        junctions_in_group: &'a rustc_hash::FxHashMap<&'a str, Vec<&'a str>>,
        child_groups: &'a rustc_hash::FxHashMap<&'a str, Vec<&'a str>>,
        service_bounds: &'a rustc_hash::FxHashMap<&'a str, Bounds>,
        junction_bounds: &'a rustc_hash::FxHashMap<&'a str, Bounds>,
    ) -> Self {
        Self {
            icon_size_px,
            padding_px,
            services_in_group,
            junctions_in_group,
            child_groups,
            service_bounds,
            junction_bounds,
            group_rects: rustc_hash::FxHashMap::default(),
            visiting: rustc_hash::FxHashSet::default(),
        }
    }

    pub(super) fn get(&self, group_id: &str) -> Option<&Bounds> {
        self.group_rects.get(group_id)
    }

    pub(super) fn compute(&mut self, group_id: &'a str) -> Option<Bounds> {
        enum Step<'a> {
            Enter(&'a str),
            Exit(&'a str),
        }

        let mut stack = vec![Step::Enter(group_id)];
        while let Some(step) = stack.pop() {
            match step {
                Step::Enter(id) => {
                    if self.group_rects.contains_key(id) {
                        continue;
                    }
                    if !self.visiting.insert(id) {
                        continue;
                    }
                    stack.push(Step::Exit(id));
                    if let Some(children) = self.child_groups.get(id) {
                        for child in children.iter().rev() {
                            if !self.group_rects.contains_key(child)
                                && !self.visiting.contains(child)
                            {
                                stack.push(Step::Enter(child));
                            }
                        }
                    }
                }
                Step::Exit(id) => {
                    let mut content: Option<Bounds> = None;
                    if let Some(svcs) = self.services_in_group.get(id) {
                        for svc_id in svcs {
                            if let Some(b) = self.service_bounds.get(svc_id) {
                                let mut tmp = content;
                                extend_bounds(&mut tmp, b.clone());
                                content = tmp;
                            }
                        }
                    }
                    if let Some(junctions) = self.junctions_in_group.get(id) {
                        for junction_id in junctions {
                            if let Some(b) = self.junction_bounds.get(junction_id) {
                                let mut tmp = content;
                                extend_bounds(&mut tmp, b.clone());
                                content = tmp;
                            }
                        }
                    }
                    if let Some(children) = self.child_groups.get(id) {
                        for child in children {
                            let Some(b) = self.group_rects.get(child).cloned() else {
                                continue;
                            };
                            let mut tmp = content;
                            extend_bounds(&mut tmp, b);
                            content = tmp;
                        }
                    }

                    // Upstream Mermaid draws group rectangles from Cytoscape `node.boundingBox()`
                    // values and then offsets them by `halfIconSize`. The helper applies the
                    // parent border and final bounding-box expansion phases after content union.
                    let pad = architecture_svg_group_bbox_padding_px(self.padding_px);
                    let b = if let Some(content) = content {
                        Bounds {
                            min_x: content.min_x - pad,
                            min_y: content.min_y - pad,
                            max_x: content.max_x + pad,
                            max_y: content.max_y + pad,
                        }
                    } else {
                        // Empty group: match Mermaid's "no children" fallback sizing behavior.
                        Bounds {
                            min_x: 0.0,
                            min_y: 0.0,
                            max_x: self.icon_size_px.max(1.0),
                            max_y: self.icon_size_px.max(1.0),
                        }
                    };
                    self.group_rects.insert(id, b);
                    self.visiting.remove(id);
                }
            };
        }

        self.group_rects.get(group_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_rect_computer_handles_deep_child_group_chain_with_small_stack() {
        const DEPTH: usize = 2048;
        let handle = std::thread::Builder::new()
            .name("architecture-group-rect-deep-chain".to_string())
            .stack_size(64 * 1024)
            .spawn(|| {
                let group_ids = (0..DEPTH).map(|i| format!("g{i}")).collect::<Vec<_>>();
                let mut services_in_group = rustc_hash::FxHashMap::<&str, Vec<&str>>::default();
                let junctions_in_group = rustc_hash::FxHashMap::<&str, Vec<&str>>::default();
                let mut child_groups = rustc_hash::FxHashMap::<&str, Vec<&str>>::default();
                let mut service_bounds = rustc_hash::FxHashMap::<&str, Bounds>::default();
                let junction_bounds = rustc_hash::FxHashMap::<&str, Bounds>::default();

                for pair in group_ids.windows(2) {
                    child_groups.insert(pair[0].as_str(), vec![pair[1].as_str()]);
                }
                services_in_group.insert(group_ids[DEPTH - 1].as_str(), vec!["leaf"]);
                service_bounds.insert(
                    "leaf",
                    Bounds {
                        min_x: 0.0,
                        min_y: 0.0,
                        max_x: 80.0,
                        max_y: 80.0,
                    },
                );

                let mut computer = GroupRectComputer::new(
                    80.0,
                    40.0,
                    &services_in_group,
                    &junctions_in_group,
                    &child_groups,
                    &service_bounds,
                    &junction_bounds,
                );
                let root = computer
                    .compute(group_ids[0].as_str())
                    .expect("root bounds");
                assert!(root.max_x > root.min_x);
                assert!(computer.get(group_ids[DEPTH - 1].as_str()).is_some());
            })
            .expect("spawn architecture group rect deep-chain test");
        handle
            .join()
            .expect("group rect deep-chain compute should not overflow");
    }
}
