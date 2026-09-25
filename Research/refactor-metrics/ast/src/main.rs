//! Read-only Rust syntax decision counter. This is not cyclomatic complexity.
use std::io::{self, Read};
use syn::visit::{self, Visit};
use syn::{Attribute, BinOp, ExprBinary, ExprClosure, ExprForLoop, ExprIf, ExprLoop, ExprMacro,
          ExprMatch, ExprTry, ExprWhile, ImplItemFn, ItemFn, ItemMacro, ItemMod, Local, StmtMacro};

#[derive(Default)]
struct Counts {
    functions: usize,
    closures: usize,
    ifs: usize,
    match_exprs: usize,
    match_arms: usize,
    match_alternatives: usize,
    match_guards: usize,
    short_circuit: usize,
    for_loops: usize,
    while_loops: usize,
    loops: usize,
    try_exprs: usize,
    let_else: usize,
    macro_boundaries: usize,
    skipped_test_items: usize,
}
impl Counts {
    fn decisions(&self) -> usize {
        self.ifs + self.match_alternatives + self.match_guards + self.short_circuit
            + self.for_loops + self.while_loops + self.loops + self.try_exprs + self.let_else
    }
    fn line(&self) -> String {
        [self.decisions(), self.functions, self.closures, self.ifs, self.match_exprs,
         self.match_arms, self.match_alternatives, self.match_guards, self.short_circuit,
         self.for_loops, self.while_loops, self.loops, self.try_exprs, self.let_else,
         self.macro_boundaries, self.skipped_test_items]
            .map(|n| n.to_string()).join("\t")
    }
}
fn test_only(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("test") ||
        (attr.path().is_ident("cfg") && match &attr.meta {
            syn::Meta::List(list) => list.tokens.to_string().split(|c: char| !c.is_alphanumeric() && c != '_').any(|word| word == "test"),
            _ => false,
        })
    })
}
impl<'ast> Visit<'ast> for Counts {
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        if test_only(&node.attrs) { self.skipped_test_items += 1; return; }
        visit::visit_item_mod(self, node);
    }
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        if test_only(&node.attrs) { self.skipped_test_items += 1; return; }
        self.functions += 1;
        visit::visit_item_fn(self, node);
    }
    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        if test_only(&node.attrs) { self.skipped_test_items += 1; return; }
        self.functions += 1;
        visit::visit_impl_item_fn(self, node);
    }
    fn visit_expr_closure(&mut self, node: &'ast ExprClosure) {
        self.closures += 1;
        visit::visit_expr_closure(self, node);
    }
    fn visit_expr_if(&mut self, node: &'ast ExprIf) {
        self.ifs += 1;
        visit::visit_expr_if(self, node);
    }
    fn visit_expr_match(&mut self, node: &'ast ExprMatch) {
        self.match_exprs += 1;
        self.match_arms += node.arms.len();
        self.match_alternatives += node.arms.len().saturating_sub(1);
        self.match_guards += node.arms.iter().filter(|arm| arm.guard.is_some()).count();
        visit::visit_expr_match(self, node);
    }
    fn visit_expr_binary(&mut self, node: &'ast ExprBinary) {
        if matches!(node.op, BinOp::And(_) | BinOp::Or(_)) { self.short_circuit += 1; }
        visit::visit_expr_binary(self, node);
    }
    fn visit_expr_for_loop(&mut self, node: &'ast ExprForLoop) {
        self.for_loops += 1;
        visit::visit_expr_for_loop(self, node);
    }
    fn visit_expr_while(&mut self, node: &'ast ExprWhile) {
        self.while_loops += 1;
        visit::visit_expr_while(self, node);
    }
    fn visit_expr_loop(&mut self, node: &'ast ExprLoop) {
        self.loops += 1;
        visit::visit_expr_loop(self, node);
    }
    fn visit_expr_try(&mut self, node: &'ast ExprTry) {
        self.try_exprs += 1;
        visit::visit_expr_try(self, node);
    }
    fn visit_local(&mut self, node: &'ast Local) {
        if node.init.as_ref().is_some_and(|init| init.diverge.is_some()) { self.let_else += 1; }
        visit::visit_local(self, node);
    }
    fn visit_expr_macro(&mut self, _node: &'ast ExprMacro) { self.macro_boundaries += 1; }
    fn visit_stmt_macro(&mut self, _node: &'ast StmtMacro) { self.macro_boundaries += 1; }
    fn visit_item_macro(&mut self, _node: &'ast ItemMacro) { self.macro_boundaries += 1; }
}
fn main() {
    let mut source = String::new();
    io::stdin().read_to_string(&mut source).expect("read stdin");
    let file = syn::parse_file(&source).expect("parse Rust source");
    let mut counts = Counts::default();
    counts.visit_file(&file);
    println!("{}", counts.line());
}
