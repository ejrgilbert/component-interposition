use crate::model::{ComponentNode, CompositionGraph, InterfaceConnection};
use std::collections::HashMap;
use wirm::wasmparser::collections::IndexSet;

const INST_PREFIX: &str = "my";
use crate::parse::config::SpliceRule;

struct Chain {
    interface: String,
    chain: Vec<u32>,
    // middlewares to inject after the specified index in the chain
    middleware_plan: HashMap<usize, IndexSet<String>>  // chain_idx -> set of middlewares to inject AFTER
}

/// Generate WAC from a composition graph and a set of splicing rules.
pub fn generate_wac(
    composition: &CompositionGraph,
    rules: &[SpliceRule],
) -> String {
    let mut wac_lines = vec!["package example:composition;".to_string()];

    // construct all the chains in the component
    let mut chains = vec![];
    for (outer_node_id, node) in composition.nodes.iter() {
        for InterfaceConnection {interface_name, source_instance, ..} in node.imports.iter() {
            let mut chain = vec![*outer_node_id];
            let mut current_id = *source_instance;
            while let Some(node) = composition.nodes.get(&current_id) {
                chain.push(current_id);
                if let Some(conn) = node.imports.iter().find(|c| c.interface_name == *interface_name) {
                    if !conn.is_host_import {
                        let src_id = conn.source_instance;
                        chain.push(src_id);
                        current_id = src_id;
                        continue;
                    }
                }
                break;
            }

            chain.reverse();
            if chain.len() > 1 { chains.push(Chain {
                interface: interface_name.to_string(),
                chain,
                middleware_plan: HashMap::new()
            }) }
        }
    }

    for Chain {interface: chain_interface, chain, middleware_plan} in chains.iter_mut() {
        for (i, window) in chain.windows(2).enumerate() {
            let inner_id = window[0];
            let outer_id = window[1];
            let inner_node = &composition.nodes[&inner_id];
            let outer_node = &composition.nodes[&outer_id];

            let inner_var = get_name(inner_node).to_string();
            let outer_var = get_name(outer_node).to_string();
            for rule in rules.iter() {
                if let SpliceRule::Between { interface, inner, outer, middlewares } = rule {
                    if interface != chain_interface { continue; }
                    if *inner == inner_var && *outer == outer_var {
                        // matches! We want to inject BEFORE the outer's index
                        middleware_plan.entry(i + 1).or_insert(
                            IndexSet::from_iter(middlewares.iter().cloned())
                        ).extend(middlewares.iter().cloned());
                    }
                }
            }
        }

        for (i, id) in chain.iter().enumerate() {
            for rule in rules {
                if let SpliceRule::Inject { interface, provider_name, middlewares } = rule {
                    if interface != chain_interface { continue; }
                    if let Some(provider) = provider_name {
                        let outer_node = &composition.nodes[id];
                        if get_name(outer_node) == *provider {
                            // matches! We want to inject BEFORE the instance this guy's plugged into
                            middleware_plan.entry(i + 1).or_insert(
                                IndexSet::from_iter(middlewares.iter().cloned())
                            ).extend(middlewares.iter().cloned());
                        }
                    }
                }
            }
        }
    }

    // Let's now generate WAC to handle the chains we've planned to emit
    let mut mdl_override = None;
    let mut last;
    let mut instance_vars: HashMap<u32, String> = HashMap::new();
    for Chain {interface: chain_interface, chain, middleware_plan} in chains.iter() {
        for (i, id) in chain.iter().enumerate() {
            let node = &composition.nodes[id];
            let node_var = get_or_create_inst(*id, node, &mut instance_vars, &mdl_override, &mut wac_lines);

            // set up what to wire in next
            last = node_var;
            mdl_override = Some((chain_interface.clone(), last.clone()));

            // if the NEXT node has a middleware BEFORE it, inject here!
            if let Some(middlewares) = middleware_plan.get(&(i + 1)) {
                for mdl in middlewares.iter() {
                    // instantiate
                    last = create_mdl(&last, mdl, chain_interface, &mut wac_lines);
                    mdl_override = Some((chain_interface.clone(), last.clone()));
                }
            }
        }
    }

    // Generate WAC to export the appropriate functions
    for (export_name, outer_inst_id) in composition.component_exports.iter() {
        let outer_node = &composition.nodes[outer_inst_id];
        let node_var = get_or_create_inst(*outer_inst_id, outer_node, &mut instance_vars, &None, &mut wac_lines);

        let export_line = format!("export {node_var}[\"{export_name}\"];");
        wac_lines.push(export_line);
    }

    wac_lines.join("\n\n")
}

fn get_or_create_inst(inst_id: u32, node: &ComponentNode, instance_vars: &mut HashMap<u32, String>, with_override: &Option<(String, String)>, wac_lines: &mut Vec<String>) -> String {
    if let Some(var) = instance_vars.get(&inst_id) {
        return var.clone();
    }
    // it hasn't been instantiated yet! do so here
    let node_var = instance_vars.entry(inst_id).or_insert_with(|| get_name(node).to_string()).clone();

    let mut line = format!("let {var} = new {INST_PREFIX}:{var} {{", var=node_var);
    for conn in &node.imports {
        if !conn.is_host_import {
            let src_id = conn.source_instance;
            if let Some((override_interface, override_var)) = &with_override {
                let src_var = if conn.interface_name == *override_interface {
                    override_var.clone()
                } else if let Some(src_var) = instance_vars.get(&src_id) {
                    // could be an import from the host!
                    // only do this if it's not
                    src_var.clone()
                } else {
                    continue;
                };
                line.push_str(&format!("\n    \"{iface}\": {src}[\"{iface}\"],", iface=conn.interface_name, src=src_var));
            }
        }
    }
    line.push_str("\n    ...\n};");
    wac_lines.push(line);

    node_var
}

fn create_mdl(input_inst: &String, mw: &String, interface: &String, wac_lines: &mut Vec<String>) -> String {
    let mw_var = mw.replace("-", "_").to_string();
    let mw_line = format!(
        "let {mw_var} = new {INST_PREFIX}:{mw} {{\n    \"{interface}\": {input_inst}[\"{interface}\"],\n}};"
    );
    wac_lines.push(mw_line);

    mw_var
}

/// Helper to get the instance name from a node
fn get_name(node: &ComponentNode) -> &str {
    node.display_label()
}
