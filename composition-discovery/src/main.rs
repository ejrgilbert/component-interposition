use std::env;
use wirm::{Component};
use wirm::ir::component::refs::{GetItemRef, GetItemRefs};
use wirm::ir::component::visitor::{traverse_component, ComponentVisitor, ResolvedItem, VisitCtx};
use wirm::wasmparser::{ComponentAlias, ComponentExport, ComponentInstance};
use crate::model::{ComponentNode, CompositionGraph, InterfaceConnection};
use crate::output::DetailLevel;

mod model;
mod ascii;
mod output;

fn main() -> Result<(), std::io::Error> {
    let Some(app_wasm_path) = env::args().nth(1) else {
        panic!("usage: discovery <composition.wasm>");
    };

    let buff = std::fs::read(app_wasm_path)?;
    let mut component = Component::parse(&buff, false, false).expect("Unable to parse");
    let mut visitor = Visitor::new();

    traverse_component(&mut component, &mut visitor);
    let res = visitor.get_results(DetailLevel::HandlerChain);

    println!("{res}");
    Ok(())
}

struct Visitor {
    first_component_instance_id: u32,
    graph: CompositionGraph,
}
impl Visitor {
    pub fn new() -> Self {
        Self {
            first_component_instance_id: u32::MAX,
            graph: CompositionGraph::new(),
        }
    }
    pub fn get_results(&mut self, detail: DetailLevel) -> String {
        // Mark host imports on the connections
        // Instances 0 to (first component instance - 1) are imports from the host
        let first_inst_id = if self.first_component_instance_id == u32::MAX {
            0
        } else {
            self.first_component_instance_id
        };

        for node in self.graph.nodes.values_mut() {
            for import in &mut node.imports {
                if let Some(source_idx) = import.source_instance {
                    if source_idx < first_inst_id {
                        import.is_host_import = true;
                    }
                }
            }
        }

        ascii::generate_ascii(&self.graph, detail)
    }
}
impl ComponentVisitor for Visitor {
    // Process component instances - ** this is where the composition wiring lives **
    fn visit_comp_instance(&mut self, cx: &VisitCtx, id: u32, instance: &ComponentInstance) {
        // Finding the ID of the first component instance (will have the smallest id)
        if id < self.first_component_instance_id {
            self.first_component_instance_id = id;
        }

        let name = cx.lookup_comp_inst_name(id)
            .map(|n| n.to_string())
            .unwrap_or_else(|| format!("instance_{}", id));
        match instance {
            ComponentInstance::Instantiate {
                component_index,
                args,
            } => {
                let mut node = ComponentNode::new(name, *component_index);

                // Process the "with" arguments - these are the interface connections
                for arg in args.iter() {
                    let interface_name = arg.name.to_string();

                    // The arg.index is the instance providing this interface
                    // It might be an alias, so resolve it to the actual source instance
                    let item = cx.resolve(&arg.get_item_ref().ref_);
                    match item {
                        ResolvedItem::CompInst(inst_id, _) => {
                            let connection = InterfaceConnection::from_instance(interface_name, inst_id);
                            node.add_import(connection);
                        },
                        ResolvedItem::Alias(alias) => {
                            let inst_ref = alias.get_item_ref();
                            if let ResolvedItem::CompInst(inst_id, _) = cx.resolve(&inst_ref.ref_) {
                                let connection = InterfaceConnection::from_instance(interface_name, inst_id);
                                node.add_import(connection);
                            }
                        },
                        _ => {}
                    }
                }

                self.graph.add_node(id, node);
            }
            ComponentInstance::FromExports(_) => {
                // This is a synthetic instance created from exports
                // These often wrap host imports - we don't track them as nodes
                // since they're just interface bundles, not actual components
            }
        }
    }
    fn visit_comp_export(&mut self, cx: &VisitCtx, export: &ComponentExport) {
        let export_name = export.name.0.to_string();
        let item = cx.resolve(&export.get_item_ref().ref_);

        // Only track instance exports
        match item {
            ResolvedItem::CompInst(inst_id, _) => {
                self.graph.add_export(export_name, inst_id);
            },
            ResolvedItem::Alias(alias) => {
                let inst_ref = alias.get_item_ref();
                if let ResolvedItem::CompInst(inst_id, _) = cx.resolve(&inst_ref.ref_) {
                    self.graph.add_export(export_name, inst_id);
                }
            },
            _ => {}
        }
    }
}
