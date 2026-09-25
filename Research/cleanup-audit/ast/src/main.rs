//! Read-only function-body similarity scanner for Rust source trees.
use proc_macro2::{TokenStream, TokenTree};
use quote::ToTokens;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use syn::visit::{self, Visit};
use syn::{Attribute, ImplItemFn, ItemFn, ItemImpl, ItemMod};

struct Body {
    path: String,
    name: String,
    line: usize,
    test: bool,
    exact: String,
    normalized: String,
    tokens: Vec<String>,
}

fn is_test(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("test")
            || (a.path().is_ident("cfg")
                && a.meta
                    .to_token_stream()
                    .to_string()
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .any(|s| s == "test"))
    })
}

fn keyword(s: &str) -> bool {
    matches!(
        s,
        "as" | "async"
            | "await"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "union"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "yield"
    )
}

fn flatten(stream: TokenStream, out: &mut Vec<String>, normalize: bool) {
    for token in stream {
        match token {
            TokenTree::Group(group) => {
                let (open, close) = match group.delimiter() {
                    proc_macro2::Delimiter::Parenthesis => ("(", ")"),
                    proc_macro2::Delimiter::Brace => ("{", "}"),
                    proc_macro2::Delimiter::Bracket => ("[", "]"),
                    proc_macro2::Delimiter::None => ("<none>", "</none>"),
                };
                out.push(open.into());
                flatten(group.stream(), out, normalize);
                out.push(close.into());
            }
            TokenTree::Ident(id) => {
                let s = id.to_string();
                out.push(if normalize && !keyword(&s) {
                    "IDENT".into()
                } else {
                    s
                });
            }
            TokenTree::Literal(lit) => out.push(if normalize {
                "LITERAL".into()
            } else {
                lit.to_string()
            }),
            TokenTree::Punct(p) => out.push(p.as_char().to_string()),
        }
    }
}

struct Collector<'a> {
    path: &'a str,
    test: bool,
    bodies: Vec<Body>,
}

impl Collector<'_> {
    fn add(&mut self, name: String, line: usize, block: &syn::Block, test: bool) {
        let stream = block.to_token_stream();
        let mut exact = Vec::new();
        let mut tokens = Vec::new();
        flatten(stream.clone(), &mut exact, false);
        flatten(stream, &mut tokens, true);
        self.bodies.push(Body {
            path: self.path.into(),
            name,
            line,
            test,
            exact: exact.join(" "),
            normalized: tokens.join(" "),
            tokens,
        });
    }
}

impl<'ast> Visit<'ast> for Collector<'_> {
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        let prior = self.test;
        self.test |= is_test(&node.attrs);
        visit::visit_item_mod(self, node);
        self.test = prior;
    }
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        self.add(
            node.sig.ident.to_string(),
            node.sig.ident.span().start().line,
            &node.block,
            self.test || is_test(&node.attrs),
        );
        // Function bodies are counted once; nested functions are uncommon and excluded.
    }
    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        let prior = self.test;
        self.test |= is_test(&node.attrs);
        visit::visit_item_impl(self, node);
        self.test = prior;
    }
    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        self.add(
            node.sig.ident.to_string(),
            node.sig.ident.span().start().line,
            &node.block,
            self.test || is_test(&node.attrs),
        );
    }
}

fn files(root: &Path, out: &mut Vec<PathBuf>) {
    if root.is_file() {
        if root.extension().is_some_and(|e| e == "rs") {
            out.push(root.into());
        }
    } else if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .file_name()
                .is_some_and(|n| n == "target" || n == ".git")
            {
                continue;
            }
            files(&path, out);
        }
    }
}

fn describe(b: &Body) -> String {
    format!(
        "{}:{}:{} [{}]",
        b.path,
        b.line,
        b.name,
        if b.test { "test" } else { "prod" }
    )
}

fn groups(bodies: &[Body], normalized: bool, minimum: usize) -> Vec<(usize, Vec<usize>)> {
    let mut map: HashMap<(&str, bool), Vec<usize>> = HashMap::new();
    for (i, b) in bodies.iter().enumerate() {
        if b.tokens.len() >= minimum {
            map.entry((if normalized { &b.normalized } else { &b.exact }, b.test))
                .or_default()
                .push(i);
        }
    }
    let mut results: Vec<_> = map
        .into_values()
        .filter(|v| v.len() > 1)
        .map(|v| (bodies[v[0]].tokens.len(), v))
        .collect();
    results.sort_by(|a, b| {
        (b.0 * (b.1.len() - 1))
            .cmp(&(a.0 * (a.1.len() - 1)))
            .then_with(|| {
                let mut am: Vec<_> = a.1.iter().map(|&i| describe(&bodies[i])).collect();
                let mut bm: Vec<_> = b.1.iter().map(|&i| describe(&bodies[i])).collect();
                am.sort();
                bm.sort();
                am.cmp(&bm)
            })
    });
    results
}

fn shingles(tokens: &[String]) -> BTreeSet<String> {
    tokens.windows(5).map(|w| w.join(" ")).collect()
}

fn main() {
    let roots: Vec<_> = env::args().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: byakko_similarity_audit <source-dir-or-file> ...");
        std::process::exit(2);
    }
    let mut paths = Vec::new();
    for root in roots {
        files(&root, &mut paths);
    }
    paths.sort();
    paths.dedup();
    let mut bodies = Vec::new();
    let mut errors = Vec::new();
    for path in &paths {
        let Ok(source) = fs::read_to_string(path) else {
            errors.push(path.display().to_string());
            continue;
        };
        let Ok(file) = syn::parse_file(&source) else {
            errors.push(path.display().to_string());
            continue;
        };
        let display = path.to_string_lossy();
        let mut collector = Collector {
            path: &display,
            test: path.components().any(|c| c.as_os_str() == "tests"),
            bodies: Vec::new(),
        };
        collector.visit_file(&file);
        bodies.extend(collector.bodies);
    }
    println!(
        "files={} functions={} parse_errors={}",
        paths.len(),
        bodies.len(),
        errors.len()
    );
    for path in errors {
        println!("PARSE_ERROR {path}");
    }
    for (title, normalized, min) in [("EXACT", false, 80), ("NORMALIZED", true, 120)] {
        println!("\n{title} groups (minimum {min} tokens)");
        for (size, members) in groups(&bodies, normalized, min).into_iter().take(30) {
            println!("tokens={size} members={}", members.len());
            for i in members {
                println!("  {}", describe(&bodies[i]));
            }
        }
    }
    let eligible: Vec<_> = bodies
        .iter()
        .enumerate()
        .filter(|(_, b)| b.tokens.len() >= 120)
        .collect();
    let sets: BTreeMap<_, _> = eligible
        .iter()
        .map(|(i, b)| (*i, shingles(&b.tokens)))
        .collect();
    let mut near = Vec::new();
    for (pos, &(i, a)) in eligible.iter().enumerate() {
        for &(j, b) in &eligible[pos + 1..] {
            if a.test != b.test || a.normalized == b.normalized {
                continue;
            }
            let (small, large) = (
                a.tokens.len().min(b.tokens.len()),
                a.tokens.len().max(b.tokens.len()),
            );
            if small * 100 < large * 70 {
                continue;
            }
            let sa = &sets[&i];
            let sb = &sets[&j];
            let intersection = sa.intersection(sb).count();
            let score = 2.0 * intersection as f64 / (sa.len() + sb.len()) as f64;
            if score >= 0.70 {
                near.push((score, i, j));
            }
        }
    }
    near.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then_with(|| describe(&bodies[a.1]).cmp(&describe(&bodies[b.1])))
            .then_with(|| describe(&bodies[a.2]).cmp(&describe(&bodies[b.2])))
    });
    for test in [false, true] {
        println!(
            "\nNEAR {} normalized 5-token shingle Dice >= 0.70 (top 30)",
            if test { "test" } else { "production" }
        );
        for &(score, i, j) in near
            .iter()
            .filter(|(_, i, _)| bodies[*i].test == test)
            .take(30)
        {
            println!(
                "{score:.3} tokens={}/{} {} <> {}",
                bodies[i].tokens.len(),
                bodies[j].tokens.len(),
                describe(&bodies[i]),
                describe(&bodies[j])
            );
        }
    }
}
