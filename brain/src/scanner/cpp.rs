//! C++ lexical scanner: classes/structs, functions, `#include` edges, and
//! brace-scoped call edges.

use regex::Regex;
use std::sync::LazyLock;

use crate::model::{Edge, Symbol};

use super::LanguageScanner;
use super::common::{import_edge, is_call_noise, make_symbol, scan_scoped_calls};

/// Matches a class/struct head, skipping any leading ALL-CAPS export macros
/// (UE's `LYRAGAME_API`, `ENGINE_API`, …) so the *class name* is captured, not
/// the macro. Group 1 = keyword, group 2 = name. Whether it is a real definition
/// or a forward declaration is decided separately by looking at what follows the
/// name (see `symbol_of`).
static CPP_CLASS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(class|struct)\s+(?:[A-Z][A-Z0-9_]+\s+)*([A-Za-z_]\w*)").unwrap()
});
static CPP_FUNCTION: LazyLock<Regex> = LazyLock::new(|| {
    // Group 1 = optional `Class::` qualifiers, group 2 = the function name.
    // Out-of-line definitions (`void UWeapon::Fire()`) are recorded with
    // their real qualified name — the weak-fingerprint link between a
    // declaration `void Fire();` and its definition `void UWeapon::Fire()`.
    Regex::new(
        r"(?:^|[;{}])\s*(?:virtual\s+|static\s+|inline\s+)?[\w:<>,*&~]+\s+((?:\w+::)*)(\w+)\s*\([^;{}]*\)",
    )
    .unwrap()
});
static INCLUDE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"#include\s+[<"]([^>"]+)[>"]"#).unwrap());
/// Recognises a function *definition* head: an optional return type and/or one
/// or more `Class::` qualifiers, then the function name and parameter list, then
/// optional trailing qualifiers and an optional `{`. Requires either a return
/// type or a `::` qualifier, which excludes bare macro invocations like
/// `GENERATED_BODY()`.
static CPP_DEFN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?:[\w<>,*&~]+\s+(?:\w+::)*|(?:\w+::)+)([A-Za-z_]\w*)\s*\([^;{}]*\)\s*(?:const\b\s*)?(?:override\b\s*)?(?:noexcept\b\s*)?(?:\{.*)?$",
    )
    .unwrap()
});

pub(crate) struct CppScanner;

impl LanguageScanner for CppScanner {
    fn scan(&self, content: &str, file: &str) -> (Vec<Symbol>, Vec<Edge>) {
        let mut symbols = Vec::new();
        let mut edges = Vec::new();
        let definitions = definition_lines(content);
        for (index, line) in content.lines().enumerate() {
            let line_number = index + 1;
            if let Some(symbol) = symbol_of(line, file, line_number, &definitions) {
                symbols.push(symbol);
            }
            if let Some(target) = INCLUDE.captures(line).and_then(|cap| cap.get(1)) {
                edges.push(import_edge(file, target.as_str(), "include", line_number));
            }
        }
        scan_scoped_calls(content, file, &mut edges, signature_of);
        (symbols, edges)
    }
}

fn symbol_of(
    line: &str,
    file: &str,
    line_number: usize,
    definitions: &std::collections::HashSet<usize>,
) -> Option<Symbol> {
    if let Some(captures) = CPP_CLASS.captures(line) {
        let name = captures.get(2)?;
        // Forward declaration vs. definition: `class Foo;` (name immediately
        // followed by `;`) declares a name but points nowhere useful, so it must
        // not be recorded as a definition — otherwise it shadows the real class
        // in symbol resolution. A definition instead carries a body `{` or a base
        // list `:` (possibly on the next line), i.e. anything but a bare `;`.
        let after = line[name.end()..].trim_start();
        if after.starts_with(';') {
            return None;
        }
        return Some(make_symbol(
            name.as_str(),
            captures.get(1)?.as_str(),
            "cpp",
            file,
            line_number,
            None,
            "definition",
        ));
    }
    let captures = CPP_FUNCTION.captures(line)?;
    let qualifiers = captures.get(1)?.as_str();
    let name = captures.get(2)?.as_str();
    if is_call_noise(name) {
        return None;
    }
    // Declaration vs definition, clangd-style: both are recorded, tagged, and
    // resolution prefers the definition. The definition set comes from the
    // stricter CPP_DEFN shape + next-line-`{` handling (see definition_lines).
    let role = if definitions.contains(&line_number) {
        "definition"
    } else {
        "declaration"
    };
    let mut symbol = make_symbol(
        name,
        "function",
        "cpp",
        file,
        line_number,
        Some(line.trim().into()),
        role,
    );
    symbol.qualified_name = format!("{qualifiers}{name}");
    Some(symbol)
}

/// The set of line numbers whose function head opens a body — i.e. is a
/// *definition* (`isThisDeclarationADefinition`, lexical edition). Reuses
/// `signature_of`'s strict shape (the line must end after the parameter list +
/// optional qualifiers/`{`, so a prototype `void Foo();` never matches), plus
/// the next-line-`{` case (`void Foo()` on one line, `{` on the next).
///
/// Honest blind spots (the no-compiler ceiling): multi-line signatures
/// `void Foo(\n  int a)\n{` are invisible to this single-line pass and fall
/// back to `declaration`; overloads with equal names share one name-keyed
/// resolution anyway.
fn definition_lines(content: &str) -> std::collections::HashSet<usize> {
    let raw_lines: Vec<&str> = content.lines().collect();
    let mut lines = std::collections::HashSet::new();
    for (index, raw) in raw_lines.iter().enumerate() {
        let line = raw.split("//").next().unwrap_or(raw);
        let Some((_, same_line_body)) = signature_of(line) else {
            continue;
        };
        if same_line_body {
            lines.insert(index + 1);
            continue;
        }
        let mut next = index + 1;
        while next < raw_lines.len() && raw_lines[next].trim().is_empty() {
            next += 1;
        }
        if next < raw_lines.len() && raw_lines[next].trim_start().starts_with('{') {
            lines.insert(index + 1);
        }
    }
    lines
}

/// Distinguish a function *definition* (opens a body) from a prototype/call.
/// A definition ends in the parameter list plus optional qualifiers/`{`; a
/// prototype ends in `;` and is rejected because it never reaches end-of-line
/// or an opening brace after the signature.
fn signature_of(line: &str) -> Option<(String, bool)> {
    let name = CPP_DEFN.captures(line)?.get(1)?.as_str().to_string();
    if is_call_noise(&name) {
        return None;
    }
    Some((name, line.contains('{')))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_cpp_class_without_compiler() {
        let (symbols, _) = CppScanner.scan("class UWeaponInstance {};", "Source/Weapon.h");
        assert_eq!(symbols[0].name, "UWeaponInstance");
        assert_eq!(symbols[0].kind, "class");
    }

    #[test]
    fn forward_declaration_is_not_recorded_as_definition() {
        // `class Foo;` declares a name but is not a definition; recording it would
        // shadow the real class in resolution.
        let (symbols, _) = CppScanner.scan("class ULyraHealthComponent;", "Fwd.h");
        assert!(symbols.is_empty());
        let (structs, _) = CppScanner.scan("struct FGameplayTag;", "Fwd.h");
        assert!(structs.is_empty());
    }

    #[test]
    fn skips_export_macro_and_captures_real_class_name() {
        // The real definition carries a UE API macro before the class name and a
        // base list; the macro must be skipped and the true name captured.
        let (symbols, _) = CppScanner.scan(
            "class LYRAGAME_API ULyraHealthComponent : public UGameFrameworkComponent",
            "LyraHealthComponent.h",
        );
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "ULyraHealthComponent");
        assert_eq!(symbols[0].kind, "class");
    }

    #[test]
    fn single_line_definition_with_trailing_semicolon_is_kept() {
        // `class Foo {};` ends in `;` but has a body — it is a definition, not a
        // forward declaration.
        let (symbols, _) = CppScanner.scan("class FRangedWeaponFiringInput {};", "x.h");
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "FRangedWeaponFiringInput");
    }

    #[test]
    fn extracts_call_edges_inside_function_body() {
        let src = "\
void UWeapon::Fire()
{
    WeaponTrace();
    DoSingleBulletTrace();
    if (bReady)
    {
        OnDamageTarget();
    }
}
";
        let (_, edges) = CppScanner.scan(src, "Source/Weapon.cpp");
        let calls: Vec<_> = edges
            .iter()
            .filter(|e| e.relation == "call")
            .map(|e| (e.source_symbol.as_str(), e.target_symbol.as_str()))
            .collect();
        assert!(calls.contains(&("Fire", "WeaponTrace")));
        assert!(calls.contains(&("Fire", "DoSingleBulletTrace")));
        assert!(calls.contains(&("Fire", "OnDamageTarget")));
        // control-flow keywords must not become call edges
        assert!(!calls.iter().any(|(_, callee)| *callee == "if"));
    }

    #[test]
    fn function_role_distinguishes_definition_from_prototype() {
        let src = "class UWeaponInstance {};
void FireWeapon();
void UWeapon::Fire()
{
    WeaponTrace();
}
int UWeapon::Heat()
{
    return 0;
}
";
        let (symbols, _) = CppScanner.scan(src, "Source/Weapon.h");
        let role_of = |name: &str| {
            symbols
                .iter()
                .find(|s| s.name == name)
                .map(|s| s.role.as_str())
        };
        assert_eq!(role_of("UWeaponInstance"), Some("definition"));
        assert_eq!(role_of("FireWeapon"), Some("declaration"));
        assert_eq!(role_of("Fire"), Some("definition"));
        assert_eq!(role_of("Heat"), Some("definition"));
        // Out-of-line definitions carry their qualified name (weak fingerprint).
        let fire = symbols.iter().find(|s| s.name == "Fire").unwrap();
        assert_eq!(fire.qualified_name, "UWeapon::Fire");
    }

    #[test]
    fn prototype_does_not_open_a_scope() {
        // A bare declaration must not swallow following calls as its body.
        let src = "\
void Declared();
void Real()
{
    Helper();
}
";
        let (_, edges) = CppScanner.scan(src, "x.cpp");
        let calls: Vec<_> = edges
            .iter()
            .filter(|e| e.relation == "call")
            .map(|e| (e.source_symbol.clone(), e.target_symbol.clone()))
            .collect();
        assert_eq!(calls, vec![("Real".to_string(), "Helper".to_string())]);
    }
}
