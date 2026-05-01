mod bindings {
    wit_bindgen::generate!({
        world: "typed-logger-mdl",
        async: true,
        generate_all
    });
}

use crate::bindings::exports::splicer::tier2::after::Guest as AfterGuest;
use crate::bindings::exports::splicer::tier2::before::Guest as BeforeGuest;
use crate::bindings::splicer::common::types::{CallId, Cell, Field, FieldTree};

pub struct Service;

impl BeforeGuest for Service {
    async fn on_call(call: CallId, args: Vec<Field>) {
        let rendered: Vec<String> = args.iter().map(fmt_arg).collect();
        let suffix = if rendered.is_empty() {
            " ()".to_string()
        } else {
            format!(" ({})", rendered.join(", "))
        };
        println!(
            "  >> [mdl-{}#{}]{}",
            call.interface_name, call.function_name, suffix,
        );
    }
}

impl AfterGuest for Service {
    async fn on_return(call: CallId, result: Option<FieldTree>) {
        let suffix = if let Some(tree) = &result {
            format!(" --> {}", fmt_res(tree))
        } else {
            " --> ()".to_string()
        };
        println!(
            "  >> [mdl-{}#{}]{}",
            call.interface_name, call.function_name, suffix,
        );
    }
}

fn fmt_arg(f: &Field) -> String {
    let (ty, val) = cell_to_str(&f.tree, f.tree.root);
    format!("{}: {ty} = {val}", f.name)
}

fn fmt_res(tree: &FieldTree) -> String {
    let (ty, val) = cell_to_str(tree, tree.root);
    format!("({ty}: {val})")
}

/// Format the cell at `idx` into `(type_label, value)`. Recurses
/// through child indices for compound cells; reads side-table
/// entries for nominal cells. Panics on out-of-bounds lookups —
/// those signal a splicer codegen contract violation, not a user-
/// recoverable condition.
fn cell_to_str(tree: &FieldTree, idx: u32) -> (String, String) {
    let cell = tree.cells.get(idx as usize).unwrap_or_else(|| {
        panic!(
            "cell index {idx} out of bounds (cells.len() = {})",
            tree.cells.len()
        )
    });
    match cell {
        // ── Primitives ────────────────────────────────────────────
        Cell::Bool(b) => ("bool".to_string(), b.to_string()),
        Cell::Integer(i) => ("int".to_string(), i.to_string()),
        Cell::Floating(x) => ("float".to_string(), x.to_string()),
        Cell::Text(s) => ("text".to_string(), format!("{s:?}")),
        Cell::Bytes(b) => ("bytes".to_string(), format!("[{}B]", b.len())),

        // ── Structural / anonymous compounds ─────────────────────
        Cell::ListOf(children) => {
            let parts = render_children(tree, children);
            ("list".to_string(), format!("[{}]", parts.join(", ")))
        }
        Cell::TupleOf(children) => {
            let parts = render_children(tree, children);
            ("tuple".to_string(), format!("({})", parts.join(", ")))
        }
        Cell::OptionSome(child) => {
            let (_, v) = cell_to_str(tree, *child);
            ("option".to_string(), format!("some({v})"))
        }
        Cell::OptionNone => ("option".to_string(), "none".to_string()),
        Cell::ResultOk(payload) => match payload {
            Some(c) => {
                let (_, v) = cell_to_str(tree, *c);
                ("result".to_string(), format!("ok({v})"))
            }
            None => ("result".to_string(), "ok".to_string()),
        },
        Cell::ResultErr(payload) => match payload {
            Some(c) => {
                let (_, v) = cell_to_str(tree, *c);
                ("result".to_string(), format!("err({v})"))
            }
            None => ("result".to_string(), "err".to_string()),
        },

        // ── Nominal compounds (side-table-backed) ────────────────
        Cell::RecordOf(side_idx) => {
            let info = side_table_get(&tree.record_infos, *side_idx, "record_infos");
            let parts: Vec<String> = info
                .fields
                .iter()
                .map(|(name, child)| {
                    let (_, v) = cell_to_str(tree, *child);
                    format!("{name}: {v}")
                })
                .collect();
            (
                format!("record({})", info.type_name),
                format!("{{ {} }}", parts.join(", ")),
            )
        }
        Cell::FlagsSet(side_idx) => {
            let info = side_table_get(&tree.flags_infos, *side_idx, "flags_infos");
            (
                format!("flags({})", info.type_name),
                info.set_flags.join(" | "),
            )
        }
        Cell::EnumCase(side_idx) => {
            let info = side_table_get(&tree.enum_infos, *side_idx, "enum_infos");
            (format!("enum({})", info.type_name), info.case_name.clone())
        }
        Cell::VariantCase(side_idx) => {
            let info = side_table_get(&tree.variant_infos, *side_idx, "variant_infos");
            let val = match info.payload {
                Some(p) => {
                    let (_, v) = cell_to_str(tree, p);
                    format!("{}({v})", info.case_name)
                }
                None => info.case_name.clone(),
            };
            (format!("variant({})", info.type_name), val)
        }

        // ── Opaque correlation handles ───────────────────────────
        Cell::ResourceHandle(side_idx) => fmt_handle(tree, *side_idx, "resource"),
        Cell::StreamHandle(side_idx) => fmt_handle(tree, *side_idx, "stream"),
        Cell::FutureHandle(side_idx) => fmt_handle(tree, *side_idx, "future"),
    }
}

fn side_table_get<'a, T>(table: &'a [T], idx: u32, name: &'static str) -> &'a T {
    table.get(idx as usize).unwrap_or_else(|| {
        panic!(
            "{name} index {idx} out of bounds (len = {}) — splicer codegen contract violation",
            table.len()
        )
    })
}

fn render_children(tree: &FieldTree, children: &[u32]) -> Vec<String> {
    children
        .iter()
        .map(|c| {
            let (_, v) = cell_to_str(tree, *c);
            v
        })
        .collect()
}

fn fmt_handle(tree: &FieldTree, side_idx: u32, kind: &str) -> (String, String) {
    let info = side_table_get(&tree.handle_infos, side_idx, "handle_infos");
    (
        format!("{kind}({})", info.type_name),
        format!("#{}", info.id),
    )
}

bindings::export!( Service with_types_in bindings );
