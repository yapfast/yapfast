//! The dependency graph: which plugins activate before which.
//!
//! Only activation has an order; compilation doesn't. A module needs nothing
//! from its dependency to compile, so the helper compiles every module at once
//! and then activates layer by layer. A cycle, a missing plugin
//! or an incompatible version is refused when the plugin is enabled, naming
//! everyone involved, rather than half-starting it later.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::ids::PluginId;
use crate::manifest::{Version, VersionReq};

/// One plugin in the graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphNode {
    pub id: PluginId,
    pub version: Version,
    /// Required dependencies, by id.
    pub dependencies: BTreeMap<PluginId, VersionReq>,
    /// Dependencies used when they happen to be enabled.
    pub optional_dependencies: BTreeMap<PluginId, VersionReq>,
}

impl GraphNode {
    /// A node with no dependencies.
    pub fn new(id: PluginId, version: Version) -> Self {
        GraphNode {
            id,
            version,
            dependencies: BTreeMap::new(),
            optional_dependencies: BTreeMap::new(),
        }
    }
}

/// Why a graph can't be activated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphError {
    /// These plugins depend on each other in a loop, in the order found.
    Cycle { plugins: Vec<PluginId> },
    /// A plugin needs another that isn't installed, or isn't enabled.
    Missing { plugin: PluginId, needs: PluginId },
    /// The version installed doesn't satisfy the requirement.
    Incompatible {
        plugin: PluginId,
        needs: PluginId,
        wants: VersionReq,
        have: Version,
    },
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphError::Cycle { plugins } => {
                let names: Vec<&str> = plugins.iter().map(PluginId::as_str).collect();
                write!(
                    f,
                    "these plugins depend on each other in a loop: {}",
                    names.join(", ")
                )
            }
            GraphError::Missing { plugin, needs } => {
                write!(f, "{plugin} needs {needs}, which isn't installed")
            }
            GraphError::Incompatible {
                plugin,
                needs,
                wants,
                have,
            } => write!(f, "{plugin} needs {needs} {wants}, but {have} is installed"),
        }
    }
}

impl std::error::Error for GraphError {}

/// Activation order for `nodes`: each layer may activate in parallel, and every
/// layer comes after the ones it depends on.
///
/// Optional dependencies order a plugin only when they are in `nodes`
/// themselves. `order` is stable: within a layer, ids are sorted, so two runs of
/// the same set activate in the same order.
pub fn layers(nodes: &[GraphNode]) -> Result<Vec<Vec<PluginId>>, GraphError> {
    let by_id: BTreeMap<&PluginId, &GraphNode> =
        nodes.iter().map(|node| (&node.id, node)).collect();

    // Check every required edge before ordering anything, so a missing
    // dependency is reported as missing rather than as a cycle.
    for node in nodes {
        for (needs, wants) in &node.dependencies {
            let Some(have) = by_id.get(needs) else {
                return Err(GraphError::Missing {
                    plugin: node.id.clone(),
                    needs: needs.clone(),
                });
            };
            if !wants.matches(have.version) {
                return Err(GraphError::Incompatible {
                    plugin: node.id.clone(),
                    needs: needs.clone(),
                    wants: wants.clone(),
                    have: have.version,
                });
            }
        }
    }

    // Kahn's algorithm: count the edges into each node, and peel off the nodes
    // with none left.
    let mut waiting_on: BTreeMap<&PluginId, usize> = BTreeMap::new();
    let mut dependents: BTreeMap<&PluginId, Vec<&PluginId>> = BTreeMap::new();
    for node in nodes {
        waiting_on.entry(&node.id).or_insert(0);
        for needs in node.dependencies.keys().chain(
            node.optional_dependencies
                .keys()
                .filter(|id| by_id.contains_key(*id)),
        ) {
            *waiting_on.entry(&node.id).or_insert(0) += 1;
            dependents.entry(needs).or_default().push(&node.id);
        }
    }

    let mut ready: Vec<&PluginId> = waiting_on
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(id, _)| *id)
        .collect();
    ready.sort();

    let mut layers: Vec<Vec<PluginId>> = Vec::new();
    let mut placed = 0;
    let mut queue: VecDeque<&PluginId> = ready.into();
    while !queue.is_empty() {
        let mut layer: Vec<&PluginId> = Vec::new();
        // Everything at the front of the queue is ready now; that whole front
        // is this layer.
        for _ in 0..queue.len() {
            if let Some(id) = queue.pop_front() {
                layer.push(id);
            }
        }
        layer.sort();
        for id in &layer {
            placed += 1;
            for dependent in dependents.get(id).into_iter().flatten() {
                if let Some(count) = waiting_on.get_mut(dependent) {
                    *count -= 1;
                    if *count == 0 {
                        queue.push_back(dependent);
                    }
                }
            }
        }
        layers.push(layer.into_iter().cloned().collect());
    }

    if placed != nodes.len() {
        let stuck: Vec<PluginId> = nodes
            .iter()
            .filter(|node| !layers.iter().any(|layer| layer.contains(&node.id)))
            .map(|node| node.id.clone())
            .collect();
        return Err(GraphError::Cycle { plugins: stuck });
    }
    Ok(layers)
}

/// Everything `plugin` needs, itself included, as an activation order.
///
/// Enabling a plugin also enables what it depends on, so this is what the parent
/// hands the helper: `plugin` is last.
pub fn closure(plugin: &PluginId, nodes: &[GraphNode]) -> Result<Vec<PluginId>, GraphError> {
    let by_id: BTreeMap<&PluginId, &GraphNode> =
        nodes.iter().map(|node| (&node.id, node)).collect();
    let mut wanted: BTreeSet<&PluginId> = BTreeSet::new();
    let mut queue = VecDeque::from([plugin]);
    while let Some(id) = queue.pop_front() {
        if !wanted.insert(id) {
            continue;
        }
        let Some(node) = by_id.get(id) else {
            if id != plugin {
                // A dependency was named by another plugin; report the edge in
                // `layers`, which knows who asked for it.
                continue;
            }
            continue;
        };
        queue.extend(node.dependencies.keys());
    }
    let subset: Vec<GraphNode> = nodes
        .iter()
        .filter(|node| wanted.contains(&node.id))
        .cloned()
        .collect();
    let ordered = layers(&subset)?;
    Ok(ordered.into_iter().flatten().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(text: &str) -> PluginId {
        PluginId::parse(text).unwrap()
    }

    fn node(text: &str, deps: &[(&str, &str)]) -> GraphNode {
        let mut node = GraphNode::new(id(text), Version::new(1, 0, 0));
        for (dep, req) in deps {
            node.dependencies.insert(id(dep), req.parse().unwrap());
        }
        node
    }

    #[test]
    fn independent_plugins_share_one_layer() {
        let order = layers(&[node("a.b", &[]), node("c.d", &[]), node("e.f", &[])]).unwrap();
        assert_eq!(order.len(), 1);
        assert_eq!(order[0].len(), 3);
    }

    #[test]
    fn a_dependent_comes_a_layer_later() {
        let order = layers(&[
            node("z.top", &[("b.base", "^1")]),
            node("b.base", &[]),
            node("c.mid", &[("b.base", "^1")]),
        ])
        .unwrap();
        assert_eq!(order[0], vec![id("b.base")]);
        let second: BTreeSet<PluginId> = order[1].iter().cloned().collect();
        assert_eq!(second, BTreeSet::from([id("c.mid"), id("z.top")]));
    }

    #[test]
    fn a_chain_layers_one_per_link() {
        let order = layers(&[
            node("c.three", &[("b.two", "^1")]),
            node("b.two", &[("a.one", "^1")]),
            node("a.one", &[]),
        ])
        .unwrap();
        assert_eq!(order.len(), 3);
        assert_eq!(order[2], vec![id("c.three")]);
    }

    #[test]
    fn a_cycle_names_itself() {
        let error = layers(&[
            node("a.one", &[("b.two", "^1")]),
            node("b.two", &[("a.one", "^1")]),
        ])
        .unwrap_err();
        let GraphError::Cycle { plugins } = error else {
            panic!("a cycle should be reported as one");
        };
        assert_eq!(plugins.len(), 2);
    }

    #[test]
    fn a_missing_dependency_is_named() {
        let error = layers(&[node("a.one", &[("b.two", "^1")])]).unwrap_err();
        assert_eq!(
            error,
            GraphError::Missing {
                plugin: id("a.one"),
                needs: id("b.two")
            }
        );
        assert!(error.to_string().contains("b.two"));
    }

    #[test]
    fn an_incompatible_version_is_named_with_both() {
        let mut top = node("a.one", &[("b.two", "^2")]);
        top.version = Version::new(1, 0, 0);
        let error = layers(&[top, node("b.two", &[])]).unwrap_err();
        assert!(matches!(error, GraphError::Incompatible { .. }));
        assert!(error.to_string().contains("^2.0.0"));
    }

    #[test]
    fn a_closure_brings_what_a_plugin_needs() {
        let nodes = [
            node("c.three", &[("b.two", "^1")]),
            node("b.two", &[("a.one", "^1")]),
            node("a.one", &[]),
            node("d.other", &[]),
        ];
        let order = closure(&id("c.three"), &nodes).unwrap();
        assert_eq!(
            order,
            vec![id("a.one"), id("b.two"), id("c.three")],
            "dependencies first, the plugin last, and nothing else"
        );
    }
}
