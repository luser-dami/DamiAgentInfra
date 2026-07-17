# TODO / Technical Debt

Improvements that have been identified but deliberately deferred, recorded so
they are not forgotten.

---

## 1. Function "declaration → definition" resolution (borrowing from clangd)

### Problem
`locate` / `resolve_symbol` may pick a function's **declaration** (prototype
`void Foo();`) instead of its definition. Forward declarations of class/struct
were fixed in the scanner (commit `e1fa476`), but function prototypes are still
not distinguished.

### How clangd does it (reference)
1. **USR (Unified Symbol Resolution)**: Clang gives every symbol a semantic
   fingerprint string encoding "fully-qualified name + signature types +
   template parameters". A function's declaration and definition share the
   **exact same USR**; overloads `foo(int)` / `foo(double)` differ — linking
   works by USR equality, not by name.
2. **Index stores both fields**: every Symbol records both a
   `CanonicalDeclaration` and a `Definition` location — not either/or.
3. **Jump policy**: reference → resolve to USR → look up index → **prefer the
   Definition, fall back to the Declaration only when absent**.
4. **Definition test**: AST `FunctionDecl::isThisDeclarationADefinition()` —
   having a body means definition.

### Red-line-compatible adaptation (no compiler/AST)
- **Data model**: add a `role` column to symbols (`definition` | `declaration`);
  record **both** decl and def, tagged — stop discarding declarations (clangd's
  two-field idea).
- **Definition test (lexical approximation)**: a body-opening `{` means
  definition; ending with `;` means declaration. The lexical equivalent of
  clangd's `isThisDeclarationADefinition()`.
- **Key point — the signal already exists**: `signature_of(line) -> (name,
  same_line_body)` called by `scanner/common.rs::scan_scoped_calls` (plus its
  pending-next-line-`{` handling) **already decides whether a function has a
  body**; today it only feeds call edges, not the symbol table.
- **resolve prefers definition**: add a highest-priority ordering dimension to
  `resolve_symbol` / `locate` — `ORDER BY (role='definition') DESC, ...`, the
  equivalent of clangd's "use def when available".
- **Weak fingerprint (USR approximation)**: use "qualified name `Class::method`
  + parameter count (commas inside parens)" as the linking key. Stronger than a
  bare name, but **overloads with the same arity but different parameter types
  still cannot be told apart** (the no-compiler ceiling).

### Implementation blockers (honestly flagged)
- `scan_scoped_calls` (the brace state machine that knows about bodies) and
  `symbol_of` (line-by-line symbol extraction) are **two independent passes**.
  Tagging a symbol's role requires sharing the "does this line open a function
  body" signal — either merge the two passes (clean but invasive), or let
  `symbol_of` do a weak test (`)` ending without `;` → definition header; fast
  but inaccurate for multi-line signatures).
- **Multi-line signature blind spot**: `void Foo::Bar(\n int a)\n{` — the
  signature line cannot see the later `{`, so a lexical scheme misjudges it.
  clangd has no such problem thanks to the AST.
- **Ceiling**: without a compiler, overloads (same name, different parameter
  types) are indistinguishable; best effort is "parameter count" granularity.

### Minimum viable version (MVP)
1. Add a `role` column to symbols.
2. Tag function extraction with the approximation "ends with `)` and no `;` →
   `definition`, else `declaration`".
3. Add `role='definition' DESC` priority to `resolve_symbol` / `locate`.
4. Document the multi-line signature blind spot and the overload ceiling
   honestly; do not pretend they are solved.
