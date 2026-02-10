//! Dependency graph for step execution ordering.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::error::{BivvyError, Result};

/// How to handle skipping a step that has dependents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SkipBehavior {
    /// Skip the step and all its dependents.
    #[default]
    SkipWithDependents,
    /// Don't skip - run the step anyway.
    RunAnyway,
    /// Skip only this step, attempt to run dependents.
    SkipOnly,
}

/// Represents the dependency relationships between steps.
#[derive(Debug, Clone)]
pub struct DependencyGraph {
    /// Map of step name to its direct dependencies.
    dependencies: HashMap<String, HashSet<String>>,
    /// Map of step name to steps that depend on it.
    dependents: HashMap<String, HashSet<String>>,
    /// All step names in the graph.
    steps: HashSet<String>,
}

impl DependencyGraph {
    /// Create a new dependency graph builder.
    pub fn builder() -> DependencyGraphBuilder {
        DependencyGraphBuilder::new()
    }

    /// Get the direct dependencies of a step.
    pub fn dependencies_of(&self, step: &str) -> Option<&HashSet<String>> {
        self.dependencies.get(step)
    }

    /// Get steps that depend on the given step.
    pub fn dependents_of(&self, step: &str) -> Option<&HashSet<String>> {
        self.dependents.get(step)
    }

    /// Check if a step exists in the graph.
    pub fn contains(&self, step: &str) -> bool {
        self.steps.contains(step)
    }

    /// Get all step names.
    pub fn steps(&self) -> &HashSet<String> {
        &self.steps
    }

    /// Get the number of steps in the graph.
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Check if the graph is empty.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Returns steps in topological order (dependencies before dependents).
    ///
    /// Returns an error if a cycle is detected.
    pub fn topological_order(&self) -> Result<Vec<String>> {
        // Count incoming edges for each node
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        for step in &self.steps {
            in_degree.insert(
                step.clone(),
                self.dependencies.get(step).map_or(0, |d| d.len()),
            );
        }

        // Start with nodes that have no dependencies
        let mut queue: VecDeque<String> = in_degree
            .iter()
            .filter(|(_, &degree)| degree == 0)
            .map(|(step, _)| step.clone())
            .collect();

        let mut result = Vec::with_capacity(self.steps.len());

        while let Some(step) = queue.pop_front() {
            result.push(step.clone());

            // Reduce in-degree for all dependents
            if let Some(dependents) = self.dependents.get(&step) {
                for dependent in dependents {
                    if let Some(degree) = in_degree.get_mut(dependent) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(dependent.clone());
                        }
                    }
                }
            }
        }

        // If we haven't processed all nodes, there's a cycle
        if result.len() != self.steps.len() {
            let remaining: Vec<_> = in_degree
                .iter()
                .filter(|(_, &d)| d > 0)
                .map(|(s, _)| s.clone())
                .collect();

            return Err(BivvyError::CircularDependency {
                cycle: remaining.join(" -> "),
            });
        }

        Ok(result)
    }

    /// Find a cycle in the graph, returning the path if one exists.
    pub fn find_cycle(&self) -> Option<Vec<String>> {
        #[derive(Clone, Copy, PartialEq)]
        enum State {
            Unvisited,
            Visiting,
            Visited,
        }

        let mut state: HashMap<&str, State> = self
            .steps
            .iter()
            .map(|s| (s.as_str(), State::Unvisited))
            .collect();

        let mut path: Vec<String> = Vec::new();

        fn dfs<'a>(
            node: &'a str,
            graph: &'a DependencyGraph,
            state: &mut HashMap<&'a str, State>,
            path: &mut Vec<String>,
        ) -> Option<Vec<String>> {
            state.insert(node, State::Visiting);
            path.push(node.to_string());

            if let Some(deps) = graph.dependencies.get(node) {
                for dep in deps {
                    match state.get(dep.as_str()) {
                        Some(State::Visiting) => {
                            // Found cycle - build the cycle path
                            let cycle_start = path.iter().position(|s| s == dep).unwrap();
                            let mut cycle: Vec<String> = path[cycle_start..].to_vec();
                            cycle.push(dep.clone());
                            return Some(cycle);
                        }
                        Some(State::Unvisited) | None => {
                            if let Some(cycle) = dfs(dep, graph, state, path) {
                                return Some(cycle);
                            }
                        }
                        Some(State::Visited) => {}
                    }
                }
            }

            path.pop();
            state.insert(node, State::Visited);
            None
        }

        for step in &self.steps {
            if state.get(step.as_str()) == Some(&State::Unvisited) {
                if let Some(cycle) = dfs(step, self, &mut state, &mut path) {
                    return Some(cycle);
                }
            }
        }

        None
    }

    /// Returns groups of steps that can execute in parallel.
    ///
    /// Each group contains steps whose dependencies are satisfied
    /// by all previous groups.
    pub fn parallel_groups(&self) -> Result<Vec<Vec<String>>> {
        // First ensure no cycles
        if let Some(cycle) = self.find_cycle() {
            return Err(BivvyError::CircularDependency {
                cycle: cycle.join(" -> "),
            });
        }

        let mut groups: Vec<Vec<String>> = Vec::new();
        let mut completed: HashSet<String> = HashSet::new();

        while completed.len() < self.steps.len() {
            let mut ready: Vec<String> = self
                .steps
                .iter()
                .filter(|s| !completed.contains(*s))
                .filter(|s| self.is_ready(s, &completed))
                .cloned()
                .collect();

            if ready.is_empty() {
                break;
            }

            // Sort for deterministic ordering
            ready.sort();

            completed.extend(ready.iter().cloned());
            groups.push(ready);
        }

        Ok(groups)
    }

    /// Check if a step is ready to run given completed steps.
    pub fn is_ready(&self, step: &str, completed: &HashSet<String>) -> bool {
        match self.dependencies.get(step) {
            None => true,
            Some(deps) => deps.iter().all(|d| completed.contains(d)),
        }
    }

    /// Get all transitive dependents of a step.
    ///
    /// Returns steps that depend on the given step, directly or indirectly.
    pub fn transitive_dependents(&self, step: &str) -> HashSet<String> {
        let mut result = HashSet::new();
        let mut to_visit = vec![step.to_string()];

        while let Some(current) = to_visit.pop() {
            if let Some(dependents) = self.dependents.get(&current) {
                for dep in dependents {
                    if result.insert(dep.clone()) {
                        to_visit.push(dep.clone());
                    }
                }
            }
        }

        result
    }

    /// Given a set of steps to skip, compute which additional steps must be skipped.
    pub fn compute_skips(&self, skip: &HashSet<String>, behavior: SkipBehavior) -> HashSet<String> {
        match behavior {
            SkipBehavior::RunAnyway => HashSet::new(),
            SkipBehavior::SkipOnly => skip.clone(),
            SkipBehavior::SkipWithDependents => {
                let mut result = skip.clone();
                for step in skip {
                    result.extend(self.transitive_dependents(step));
                }
                result
            }
        }
    }
}

/// Builder for constructing a DependencyGraph.
#[derive(Debug, Default)]
pub struct DependencyGraphBuilder {
    dependencies: HashMap<String, HashSet<String>>,
}

impl DependencyGraphBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a step with its dependencies.
    pub fn add_step(mut self, name: impl Into<String>, depends_on: Vec<String>) -> Self {
        let name = name.into();
        self.dependencies
            .entry(name)
            .or_default()
            .extend(depends_on);
        self
    }

    /// Build the dependency graph.
    ///
    /// Returns an error if any dependency references a non-existent step.
    pub fn build(self) -> Result<DependencyGraph> {
        // Collect all step names
        let steps: HashSet<String> = self.dependencies.keys().cloned().collect();

        // Validate all dependencies exist
        for (step, deps) in &self.dependencies {
            for dep in deps {
                if !steps.contains(dep) {
                    return Err(BivvyError::ConfigValidationError {
                        message: format!("Step '{}' depends on unknown step '{}'", step, dep),
                    });
                }
            }
        }

        // Build dependents map (reverse lookup)
        let mut dependents: HashMap<String, HashSet<String>> = HashMap::new();
        for step in &steps {
            dependents.insert(step.clone(), HashSet::new());
        }

        for (step, deps) in &self.dependencies {
            for dep in deps {
                dependents.get_mut(dep).unwrap().insert(step.clone());
            }
        }

        Ok(DependencyGraph {
            dependencies: self.dependencies,
            dependents,
            steps,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_creates_empty_graph() {
        let graph = DependencyGraph::builder().build().unwrap();
        assert!(graph.is_empty());
    }

    #[test]
    fn builder_adds_single_step_without_dependencies() {
        let graph = DependencyGraph::builder()
            .add_step("step1", vec![])
            .build()
            .unwrap();

        assert!(graph.contains("step1"));
        assert_eq!(graph.len(), 1);
    }

    #[test]
    fn builder_adds_step_with_dependencies() {
        let graph = DependencyGraph::builder()
            .add_step("step1", vec![])
            .add_step("step2", vec!["step1".to_string()])
            .build()
            .unwrap();

        assert!(graph.contains("step1"));
        assert!(graph.contains("step2"));
        assert!(graph.dependencies_of("step2").unwrap().contains("step1"));
    }

    #[test]
    fn builder_tracks_dependents() {
        let graph = DependencyGraph::builder()
            .add_step("step1", vec![])
            .add_step("step2", vec!["step1".to_string()])
            .add_step("step3", vec!["step1".to_string()])
            .build()
            .unwrap();

        let dependents = graph.dependents_of("step1").unwrap();
        assert!(dependents.contains("step2"));
        assert!(dependents.contains("step3"));
    }

    #[test]
    fn builder_rejects_unknown_dependency() {
        let result = DependencyGraph::builder()
            .add_step("step1", vec!["nonexistent".to_string()])
            .build();

        assert!(result.is_err());
    }

    #[test]
    fn topo_sort_empty_graph() {
        let graph = DependencyGraph::builder().build().unwrap();
        let order = graph.topological_order().unwrap();
        assert!(order.is_empty());
    }

    #[test]
    fn topo_sort_single_step() {
        let graph = DependencyGraph::builder()
            .add_step("step1", vec![])
            .build()
            .unwrap();

        let order = graph.topological_order().unwrap();
        assert_eq!(order, vec!["step1"]);
    }

    #[test]
    fn topo_sort_linear_chain() {
        let graph = DependencyGraph::builder()
            .add_step("first", vec![])
            .add_step("second", vec!["first".to_string()])
            .add_step("third", vec!["second".to_string()])
            .build()
            .unwrap();

        let order = graph.topological_order().unwrap();

        let first_idx = order.iter().position(|s| s == "first").unwrap();
        let second_idx = order.iter().position(|s| s == "second").unwrap();
        let third_idx = order.iter().position(|s| s == "third").unwrap();

        assert!(first_idx < second_idx);
        assert!(second_idx < third_idx);
    }

    #[test]
    fn topo_sort_diamond_dependency() {
        let graph = DependencyGraph::builder()
            .add_step("a", vec![])
            .add_step("b", vec!["a".to_string()])
            .add_step("c", vec!["a".to_string()])
            .add_step("d", vec!["b".to_string(), "c".to_string()])
            .build()
            .unwrap();

        let order = graph.topological_order().unwrap();

        let a_idx = order.iter().position(|s| s == "a").unwrap();
        let b_idx = order.iter().position(|s| s == "b").unwrap();
        let c_idx = order.iter().position(|s| s == "c").unwrap();
        let d_idx = order.iter().position(|s| s == "d").unwrap();

        assert!(a_idx < b_idx);
        assert!(a_idx < c_idx);
        assert!(b_idx < d_idx);
        assert!(c_idx < d_idx);
    }

    #[test]
    fn topo_sort_detects_simple_cycle() {
        let graph = DependencyGraph::builder()
            .add_step("a", vec!["b".to_string()])
            .add_step("b", vec!["a".to_string()])
            .build()
            .unwrap();

        let result = graph.topological_order();
        assert!(result.is_err());
    }

    #[test]
    fn no_cycle_returns_none() {
        let graph = DependencyGraph::builder()
            .add_step("a", vec![])
            .add_step("b", vec!["a".to_string()])
            .build()
            .unwrap();

        assert!(graph.find_cycle().is_none());
    }

    #[test]
    fn simple_cycle_returns_path() {
        let graph = DependencyGraph::builder()
            .add_step("a", vec!["b".to_string()])
            .add_step("b", vec!["a".to_string()])
            .build()
            .unwrap();

        let cycle = graph.find_cycle();
        assert!(cycle.is_some());

        let path = cycle.unwrap();
        // Path should show the cycle: e.g., ["a", "b", "a"]
        assert!(path.len() >= 2);
        assert_eq!(path.first(), path.last());
    }

    #[test]
    fn longer_cycle_returns_full_path() {
        let graph = DependencyGraph::builder()
            .add_step("a", vec!["c".to_string()])
            .add_step("b", vec!["a".to_string()])
            .add_step("c", vec!["b".to_string()])
            .build()
            .unwrap();

        let cycle = graph.find_cycle();
        assert!(cycle.is_some());

        let path = cycle.unwrap();
        assert!(path.contains(&"a".to_string()));
        assert!(path.contains(&"b".to_string()));
        assert!(path.contains(&"c".to_string()));
    }

    #[test]
    fn self_cycle_detected() {
        let graph = DependencyGraph::builder()
            .add_step("a", vec!["a".to_string()])
            .build()
            .unwrap();

        let cycle = graph.find_cycle();
        assert!(cycle.is_some());
    }

    #[test]
    fn parallel_groups_empty_graph() {
        let graph = DependencyGraph::builder().build().unwrap();
        let groups = graph.parallel_groups().unwrap();
        assert!(groups.is_empty());
    }

    #[test]
    fn parallel_groups_independent_steps() {
        let graph = DependencyGraph::builder()
            .add_step("a", vec![])
            .add_step("b", vec![])
            .add_step("c", vec![])
            .build()
            .unwrap();

        let groups = graph.parallel_groups().unwrap();

        // All steps have no dependencies, so they're in one group
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 3);
    }

    #[test]
    fn parallel_groups_linear_chain() {
        let graph = DependencyGraph::builder()
            .add_step("first", vec![])
            .add_step("second", vec!["first".to_string()])
            .add_step("third", vec!["second".to_string()])
            .build()
            .unwrap();

        let groups = graph.parallel_groups().unwrap();

        // Each step must be in its own group
        assert_eq!(groups.len(), 3);
    }

    #[test]
    fn parallel_groups_diamond() {
        let graph = DependencyGraph::builder()
            .add_step("a", vec![])
            .add_step("b", vec!["a".to_string()])
            .add_step("c", vec!["a".to_string()])
            .add_step("d", vec!["b".to_string(), "c".to_string()])
            .build()
            .unwrap();

        let groups = graph.parallel_groups().unwrap();

        // Group 1: [a], Group 2: [b, c], Group 3: [d]
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0], vec!["a"]);
        assert!(groups[1].contains(&"b".to_string()));
        assert!(groups[1].contains(&"c".to_string()));
        assert_eq!(groups[2], vec!["d"]);
    }

    #[test]
    fn is_ready_no_dependencies() {
        let graph = DependencyGraph::builder()
            .add_step("a", vec![])
            .build()
            .unwrap();

        assert!(graph.is_ready("a", &HashSet::new()));
    }

    #[test]
    fn is_ready_dependencies_met() {
        let graph = DependencyGraph::builder()
            .add_step("a", vec![])
            .add_step("b", vec!["a".to_string()])
            .build()
            .unwrap();

        let mut completed = HashSet::new();
        completed.insert("a".to_string());

        assert!(graph.is_ready("b", &completed));
    }

    #[test]
    fn is_ready_dependencies_not_met() {
        let graph = DependencyGraph::builder()
            .add_step("a", vec![])
            .add_step("b", vec!["a".to_string()])
            .build()
            .unwrap();

        assert!(!graph.is_ready("b", &HashSet::new()));
    }

    #[test]
    fn transitive_dependents_none() {
        let graph = DependencyGraph::builder()
            .add_step("a", vec![])
            .build()
            .unwrap();

        let deps = graph.transitive_dependents("a");
        assert!(deps.is_empty());
    }

    #[test]
    fn transitive_dependents_direct() {
        let graph = DependencyGraph::builder()
            .add_step("a", vec![])
            .add_step("b", vec!["a".to_string()])
            .build()
            .unwrap();

        let deps = graph.transitive_dependents("a");
        assert!(deps.contains("b"));
    }

    #[test]
    fn transitive_dependents_indirect() {
        let graph = DependencyGraph::builder()
            .add_step("a", vec![])
            .add_step("b", vec!["a".to_string()])
            .add_step("c", vec!["b".to_string()])
            .build()
            .unwrap();

        let deps = graph.transitive_dependents("a");
        assert!(deps.contains("b"));
        assert!(deps.contains("c"));
    }

    #[test]
    fn compute_skips_with_dependents() {
        let graph = DependencyGraph::builder()
            .add_step("a", vec![])
            .add_step("b", vec!["a".to_string()])
            .add_step("c", vec!["b".to_string()])
            .build()
            .unwrap();

        let mut skip = HashSet::new();
        skip.insert("a".to_string());

        let skipped = graph.compute_skips(&skip, SkipBehavior::SkipWithDependents);

        assert!(skipped.contains("a"));
        assert!(skipped.contains("b"));
        assert!(skipped.contains("c"));
    }

    #[test]
    fn compute_skips_only() {
        let graph = DependencyGraph::builder()
            .add_step("a", vec![])
            .add_step("b", vec!["a".to_string()])
            .build()
            .unwrap();

        let mut skip = HashSet::new();
        skip.insert("a".to_string());

        let skipped = graph.compute_skips(&skip, SkipBehavior::SkipOnly);

        assert!(skipped.contains("a"));
        assert!(!skipped.contains("b"));
    }

    #[test]
    fn compute_skips_run_anyway() {
        let graph = DependencyGraph::builder()
            .add_step("a", vec![])
            .add_step("b", vec!["a".to_string()])
            .build()
            .unwrap();

        let mut skip = HashSet::new();
        skip.insert("a".to_string());

        let skipped = graph.compute_skips(&skip, SkipBehavior::RunAnyway);

        assert!(skipped.is_empty());
    }
}
