// Copyright 2024-2025 Irreducible Inc.
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    thread::ThreadId,
    time::Instant,
};

use crate::{
    data::{EventCounts, LogTree, StoringFieldVisitor},
    env_utils::{get_bool_env_var, get_env_var},
    errors::err_msg,
};
use linear_map::LinearMap;
use tracing::span;

/// Tree layer config.
#[derive(Debug, Clone)]
pub struct Config {
    /// Display anything above this percentage in bold red.
    /// Corresponds to the `TREE_LAYER_ATTENTION_ABOVE` environment variable.
    pub attention_above_percent: f64,

    /// Display anything above this percentage in regular white.
    /// Anything below this percentage will be displayed in dim white/gray.
    /// Corresponds to the `TREE_LAYER_RELEVANT_ABOVE` environment variable.
    pub relevant_above_percent: f64,

    /// Anything below this percentage is collapsed into `[...]`.
    /// This is checked after duplicate calls below relevant_above_percent are aggregated.
    /// Corresponds to the `TREE_LAYER_HIDE_BELOW` environment variable.
    pub hide_below_percent: f64,

    /// Whether to display parent time minus time of all children as
    /// `[unaccounted]`. Useful to sanity check that you are measuring all the bottlenecks.
    /// Corresponds to the `TREE_LAYER_DISPLAY_UNACCOUNTED` environment variable.
    pub display_unaccounted: bool,

    /// Whether to accumulate events of the children into the parent.
    /// Default is true.
    /// Corresponds to the `TREE_LAYER_ACCUMULATE_EVENTS` environment variable.
    pub accumulate_events: bool,

    /// If enabled the number of spans will be added to the event information and grouped by span
    /// names. Has effect only if `accumulate_events` is enabled.
    /// Corresponds to the `TREE_LAYER_ACCUMULATE_SPANS_COUNT` environment variable.
    pub accumulate_spans_count: bool,

    /// EXPERIMENTAL:
    /// If enabled, similar sibling nodes will be aggregated together.
    /// In essence spans that share name, fields and children will be merged together.
    /// This means that the start+end time will be wrong but the durations are summed.
    pub aggregate_similar_siblings: bool,

    /// Whether to disable color output.
    /// Corresponds to the `NO_COLOR` environment variable.
    pub no_color: bool,
}

impl Config {
    fn from_env() -> Self {
        Self {
            attention_above_percent: get_env_var("TREE_LAYER_ATTENTION_ABOVE", 25.0),
            relevant_above_percent: get_env_var("TREE_LAYER_RELEVANT_ABOVE", 2.5),
            hide_below_percent: get_env_var("TREE_LAYER_HIDE_BELOW", 1.0),
            display_unaccounted: get_env_var("TREE_LAYER_DISPLAY_", false),
            accumulate_events: get_bool_env_var("TREE_LAYER_ACCUMULATE_EVENTS", true),
            accumulate_spans_count: get_bool_env_var("TREE_LAYER_ACCUMULATE_SPANS_COUNT", false),
            aggregate_similar_siblings: get_bool_env_var("TREE_LAYER_ACCUMULATE_SIBLINGS", false),
            no_color: get_bool_env_var("NO_COLOR", false),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::from_env()
    }
}

#[derive(Default)]
struct State {
    current_span: Option<span::Id>,
    unfinished_spans: LinearMap<u64, GraphNode>,
    zero_level_events: EventCounts,
}

impl State {
    fn print_zero_level_events(&mut self) {
        if !self.zero_level_events.is_empty() {
            println!("> {}\n", self.zero_level_events.format().join("\n> "));

            self.zero_level_events.clear();
        }
    }
}

pub struct Guard {
    state: Arc<Mutex<State>>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        let Ok(mut state) = self.state.lock() else {
            return err_msg!("failed to get mutex");
        };

        state.print_zero_level_events();
    }
}

/// GraphLayer (internally called layer::graph)
/// This Layer prints a call graph to stdout. Please note that this layer both prints data about spans and events.
/// Spans from the threads other than the main thread are not printed. Events from the main thread are attached to the latest main thread span.
/// Depending on the `Config::accumulate_events` setting, the layer will either print the events of each span or accumulate the events of the children into the parent.
///
/// example output:
/// ```bash
/// cargo test all_layers -- --nocapture
///
/// running 1 test
/// root span [ 178.94µs | 100.00% ]
/// Events:
///   event in span2: 1
///   event in span3 { field5: value5 }: 2
///
/// ├── child span1 [ 4.63µs | 2.59% ] { field1 = value1 }
/// └── child span2 [ 93.40µs | 52.20% ] { field2 = value2 }
///     Events:
///       event in span2: 1
///       event in span3 { field5: value5 }: 2
///     
///    ├── child span3 [ 15.47µs | 8.64% ] { field3 = value3 }
///    │   Events:
///    │     event in span3 { field5: value5 }: 2
///    │   
///    └── child span4 [ 2.87µs | 1.60% ] { field4 = value4 }
///
/// test tests::all_layers ... ok
/// ```
pub struct Layer {
    main_thread: ThreadId,
    state: Arc<Mutex<State>>,
    config: Config,
}

impl Layer {
    pub fn new(config: Config) -> (Self, Guard) {
        let state = Arc::new(Mutex::new(State::default()));
        let layer = Self {
            main_thread: std::thread::current().id(),
            state: state.clone(),
            config: config.clone(),
        };
        let guard = Guard { state };

        (layer, guard)
    }

    fn is_main_thread(&self) -> bool {
        self.main_thread == std::thread::current().id()
    }
}

impl<S> tracing_subscriber::Layer<S> for Layer
where
    S: tracing::Subscriber,
    // no idea what this is but it lets you access the parent span.
    S: for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    fn on_new_span(
        &self,
        attrs: &span::Attributes<'_>,
        id: &span::Id,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if !self.is_main_thread() {
            return;
        }

        let mut graph_node = GraphNode {
            call_count: 1,
            ..Default::default()
        };
        let mut visitor = StoringFieldVisitor(&mut graph_node.metadata);
        attrs.record(&mut visitor);

        let Ok(mut state) = self.state.lock() else {
            return err_msg!("failed to get mutex");
        };
        // RN I need this for debugging...
        graph_node.name = ctx.span(id).map(|s| s.name()).unwrap_or("unknown span");

        state.unfinished_spans.insert(id.into_u64(), graph_node);
    }

    fn on_record(
        &self,
        id: &span::Id,
        values: &span::Record<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if !self.is_main_thread() {
            return;
        }

        let Ok(mut state) = self.state.lock() else {
            return err_msg!("failed to get mutex");
        };

        if let Some(graph_node) = state.unfinished_spans.get_mut(&id.into_u64()) {
            let mut visitor = StoringFieldVisitor(&mut graph_node.metadata);
            values.record(&mut visitor);
        }
    }

    fn on_enter(&self, id: &span::Id, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        if !self.is_main_thread() {
            return;
        }

        let Ok(mut state) = self.state.lock() else {
            return err_msg!("failed to get mutex");
        };

        state.current_span = Some(id.clone());
        if let Some(graph_node) = state.unfinished_spans.get_mut(&id.into_u64()) {
            graph_node.started = Some(Instant::now());
        }

        state.print_zero_level_events();
    }

    fn on_exit(&self, id: &span::Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        if !self.is_main_thread() {
            return;
        }

        let Some(span) = ctx.span(id) else {
            return err_msg!("failed to get span on_exit");
        };

        let Ok(mut state) = self.state.lock() else {
            return err_msg!("failed to get mutex");
        };

        let mut node = state
            .unfinished_spans
            .remove(&id.into_u64())
            .unwrap_or_default();
        node.execution_duration = node
            .started
            .map(|started| Instant::elapsed(&started))
            .unwrap_or_default();
        // Q: why is the name assigned on finish and not on start?
        node.name = span.name();

        let parent = match span.parent() {
            Some(p) => {
                let Some(parent_node) = state.unfinished_spans.get_mut(&p.id().into_u64()) else {
                    return err_msg!("failed to get parent node");
                };

                if self.config.aggregate_similar_siblings {
                    node.summarize_children();
                }
                parent_node.push_node(node, self.config.aggregate_similar_siblings);
                Some(p.id().clone())
            }
            None => {
                node.print(&self.config);

                None
            }
        };

        state.current_span = parent;
    }

    fn on_event(&self, event: &tracing::Event<'_>, ctx: tracing_subscriber::layer::Context<'_, S>) {
        if event.is_root() {
            return;
        }

        let Ok(mut state) = self.state.lock() else {
            return err_msg!("failed to get mutex");
        };

        let span_id = if self.is_main_thread() {
            event
                .parent()
                .cloned()
                .or_else(|| ctx.current_span().id().cloned())
        } else {
            // try to attach the event to the latest main thread span
            state.current_span.clone()
        };

        match span_id {
            Some(span_id) => {
                if let Some(graph_node) = state.unfinished_spans.get_mut(&span_id.into_u64()) {
                    graph_node.events.record(event);
                }
            }
            None => {
                state.zero_level_events.record(event);
            }
        }
    }
}

#[derive(Default, Debug, Clone)]
struct GraphNode {
    name: &'static str,
    started: Option<Instant>,
    execution_duration: std::time::Duration,
    metadata: LinearMap<&'static str, String>,
    events: EventCounts,
    child_nodes: Vec<GraphNode>,
    call_count: usize,
}

impl GraphNode {
    fn is_equivalent(&self, other: &GraphNode) -> bool {
        let base_same = self.name == other.name && self.metadata == other.metadata;
        let children_same = self.child_nodes.len() == other.child_nodes.len()
            && self
                .child_nodes
                .iter()
                .zip(other.child_nodes.iter())
                .all(|(a, b)| a.is_equivalent(b));
        // Q: is all of 2 empty true?
        // A: it is!
        return base_same && children_same;
    }

    fn fold(&mut self, other: GraphNode) -> () {
        // Replace start with the min start time if present
        self.started = other
            .started
            .map(|s| self.started.map(|s0| s0.min(s)).unwrap_or(s))
            .or(self.started);
        self.execution_duration += other.execution_duration;
        self.call_count += other.call_count;
        self.events += &other.events;

        // TODO reimplement as drain so I dont have to clone
        for (i, left_node) in self.child_nodes.iter_mut().enumerate() {
            if let Some(right_node) = other.child_nodes.get(i) {
                if left_node.is_equivalent(right_node) {
                    left_node.fold(right_node.clone());
                } else {
                    println!("This should not happen ... equivalence check should have failed.");
                    left_node.child_nodes.push(right_node.clone());
                }
            }
        }
    }

    fn summarize_children(&mut self) {
        // In essence we check if any of out children are equivalent and if so we fold them
        // together.
        //
        // we start with the first and just iterate over the rest.

        let mut out = Vec::new();
        let init_len = self.child_nodes.len();
        while self.child_nodes.len() > 0 {
            let mut node = self.child_nodes.pop().expect("should have node");
            node.summarize_children();
            let mut i = 0;
            while i < self.child_nodes.len() {
                if node.is_equivalent(&self.child_nodes[i]) {
                    let other = self.child_nodes.remove(i);
                    node.fold(other);
                } else {
                    i += 1;
                }
            }
            out.push(node);
            if out.len() > init_len {
                // something is wrong, we should not be growing the number of nodes
                break;
            }
        }
        self.child_nodes = out;
    }

    fn push_node(&mut self, node: GraphNode, try_merge: bool) {
        if try_merge {
            for existing in self.child_nodes.iter_mut() {
                if existing.is_equivalent(&node) {
                    existing.fold(node);
                    return;
                }
            }
        }
        self.child_nodes.push(node);
    }
}

impl GraphNode {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            ..Default::default()
        }
    }

    fn execution_percentage(&self, root_time: std::time::Duration) -> f64 {
        100.0 * self.execution_duration.as_secs_f64() / root_time.as_secs_f64()
    }

    /// For each node accumulate the events of its children and return the total events.
    fn accumulate_children_events(&mut self, accumulate_spans_count: bool) {
        for child in self.child_nodes.iter_mut() {
            child.accumulate_children_events(accumulate_spans_count);

            if accumulate_spans_count {
                child.record_self_as_event();
            }

            self.events += &child.events;
        }
    }

    /// Record the span node as event.
    /// Handy to calculate the number of spans of the type.
    fn record_self_as_event(&mut self) {
        self.events.increment_events_counter(self.name);
    }

    fn print(mut self, config: &Config) {
        if config.accumulate_events {
            self.accumulate_children_events(config.accumulate_spans_count);
        }
        if config.aggregate_similar_siblings {
            self.summarize_children();
        }

        let tree = self.render_tree(self.execution_duration, config);
        println!("{tree}");
    }

    fn label(&self, root_time: std::time::Duration, config: &Config) -> String {
        let mut info = vec![];
        if self.call_count > 1 {
            info.push(format!("({} calls)", self.call_count))
        } else if !self.metadata.is_empty() {
            let kv: Vec<_> = self
                .metadata
                .iter()
                .map(|(k, v)| format!("{k} = {v}"))
                .collect();
            info.push(format!("{{ {} }}", kv.join(", ")))
        }

        let name = &self.name;
        let execution_time = self.execution_duration;
        let execution_time_percent = self.execution_percentage(root_time);
        let mut result = format!("{name} [ {execution_time:.2?} | {execution_time_percent:.2}% ]");
        if !info.is_empty() {
            result = format!("{result} {}", info.join(" "));
        }

        if config.no_color {
            result
        } else {
            format!(
                "{}{}\x1b[0m",
                if execution_time_percent > config.attention_above_percent {
                    "\x1b[1;31m" // bold red
                } else if execution_time_percent > config.relevant_above_percent {
                    "\x1b[0m" // white
                } else {
                    "\x1b[2m" // gray
                },
                result
            )
        }
    }

    fn render_tree(&self, root_time: std::time::Duration, config: &Config) -> LogTree {
        let mut children = vec![];
        let mut aggregated_node: Option<GraphNode> = None;
        let mut name_counter: HashMap<&str, usize> = HashMap::new();

        for (i, child) in self.child_nodes.iter().enumerate() {
            let name_count = name_counter.entry(child.name).or_insert(0);
            *name_count += 1;

            let next = self.child_nodes.get(i + 1);
            if next.is_some_and(|next| next.name == child.name) {
                if child.execution_percentage(root_time) > config.relevant_above_percent {
                    let mut indexed_child = child.clone();
                    indexed_child
                        .metadata
                        .insert("index", format!("{name_count}"));
                    children.push(indexed_child);
                } else {
                    aggregated_node = aggregated_node
                        .map(|node| node.clone().aggregate(child))
                        .or_else(|| Some(child.clone()));
                }
            } else {
                let child = aggregated_node.take().unwrap_or_else(|| child.clone());
                children.push(child);
            }
        }

        if config.hide_below_percent > 0.0 {
            children = children.into_iter().fold(vec![], |acc, child| {
                let mut acc = acc;
                if child.execution_percentage(root_time) < config.hide_below_percent {
                    if let Some(x) = acc.last_mut() {
                        if x.name == "[...]" {
                            *x = x.clone().aggregate(&child);
                        } else {
                            acc.push(GraphNode::new("[...]").aggregate(&child))
                        }
                    }
                } else {
                    acc.push(child);
                }
                acc
            });
        }

        if config.display_unaccounted && !children.is_empty() {
            let mut unaccounted = GraphNode::new("[unaccounted]");
            unaccounted.execution_duration = self.execution_duration
                - self
                    .child_nodes
                    .iter()
                    .map(|x| x.execution_duration)
                    .fold(std::time::Duration::new(0, 0), |x, y| x + y);

            children.insert(0, unaccounted);
        }

        LogTree {
            label: self.label(root_time, config),
            events: self.events.format(),
            children: children
                .into_iter()
                .map(|child| child.render_tree(root_time, config))
                .collect(),
        }
    }

    fn aggregate(mut self, other: &GraphNode) -> Self {
        self.execution_duration += other.execution_duration;
        self.call_count += other.call_count;
        self.events += &other.events;

        self
    }
}

#[cfg(test)]
mod tests {
    use {crate::data::CounterValue, tracing_subscriber::util::SubscriberInitExt};
    use {
        crate::{PrintTreeConfig, PrintTreeLayer},
        tracing_subscriber::layer::SubscriberExt,
    };
    use {
        std::{thread, time::Duration},
        tracing::{debug_span, event, Level},
    };

    #[test]
    fn test_summarize_nodes() {
        // TODO:Add test for aggregation of nodes ...
        use std::time::Duration;
        let mut root = GraphNode::new("root");
    }

    #[test]
    fn test_incremental_events_counts() {
        let (layer, guard) = PrintTreeLayer::new(PrintTreeConfig::default());
        let layer = tracing_subscriber::registry().with(layer);
        layer.try_init().unwrap();

        let span = debug_span!("root span");
        let _scope1 = span.enter();
        thread::sleep(Duration::from_millis(20));
        event!(name: "proof_size", Level::INFO, counter=true, incremental=true, value=1);
        // child spans 1 and 2 are siblings
        let span2 = debug_span!("child span1", field1 = "value1", perfetto_track_id = 5);
        let scope2 = span2.enter();
        thread::sleep(Duration::from_millis(20));
        drop(scope2);

        let span3 = debug_span!(
            "child span2",
            field2 = "value2",
            value = 20,
            perfetto_track_id = 5,
            perfetto_flow_id = 10
        );
        let _scope3 = span3.enter();

        thread::sleep(Duration::from_millis(20));
        event!(name: "proof_size", Level::INFO, counter=true, incremental=true, value=3);

        // child spans 3 and 4 are siblings
        let span = debug_span!("child span3", field3 = "value3");
        let scope = span.enter();
        thread::sleep(Duration::from_millis(20));
        event!(name: "custom event", Level::DEBUG, {field5 = "value5", counter = true, value = 30});
        drop(scope);

        thread::spawn(|| {
            let span = debug_span!("child span5", field5 = "value5");
            let _scope = span.enter();
            thread::sleep(Duration::from_millis(20));
            event!(name: "proof_size", Level::INFO, counter=true, incremental=true, value=6);
        })
        .join()
        .unwrap();

        let span = debug_span!("child span4", field4 = "value4", perfetto_flow_id = 10);
        thread::sleep(Duration::from_millis(20));
        event!(name: "custom event", Level::DEBUG, {field5 = "value5", counter = true, value = 40});
        let scope = span.enter();
        thread::sleep(Duration::from_millis(20));
        drop(scope);
        drop(_scope3);

        let mut state = guard.state.lock().unwrap();
        let root = state.unfinished_spans.get_mut(&1).unwrap();

        root.accumulate_children_events(true);

        assert_eq!(
            *root.events.get("proof_size").unwrap(),
            CounterValue::Int(10)
        );

        // remove to avoid an incorrect graph print
        state.unfinished_spans.remove(&1).unwrap();
    }
}
