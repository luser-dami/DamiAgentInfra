//! End-to-end integration tests: a real temporary project scanned, compiled
//! and queried through the actual `alexandria` binary. These guard the
//! promises unit tests cannot see — the full pipeline, multi-library fusion,
//! pack late binding, and the feedback loop.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// Absolute path to the binary under test (cargo provides it for
/// integration tests; fall back to the conventional target location).
fn alexandria_bin() -> PathBuf {
    if let Some(path) = option_env!("CARGO_BIN_EXE_alexandria") {
        return PathBuf::from(path);
    }
    let mut path = std::env::current_exe().unwrap();
    path.pop(); // deps/
    path.pop(); // release/ or debug/
    path.push("alexandria");
    path.set_extension("exe");
    path
}

fn library(project: &Path, args: &[&str]) -> (bool, String, String) {
    let output = Command::new(alexandria_bin())
        .arg("--project-root")
        .arg(project)
        .args(args)
        .output()
        .expect("failed to run alexandria");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

fn json_stdout(project: &Path, args: &[&str]) -> Value {
    let (ok, stdout, stderr) = library(project, args);
    assert!(ok, "command failed: {args:?}\n{stderr}");
    serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("bad json from {args:?}: {e}\n{stdout}"))
}

/// A unique temp project directory per test.
fn temp_project(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("alexandria_it_{}_{}", name, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Write a minimal but complete project: two C++ files (a declaration in the
/// header, a qualified definition in the .cpp), project config, and one
/// module document citing the real class location.
fn seed_project(root: &Path) {
    fs::create_dir_all(root.join("Source/Game")).unwrap();
    fs::write(
        root.join("Source/Game/Weapon.h"),
        "#pragma once\n\
         class UWeapon\n\
         {\n\
         public:\n\
         \tvoid Fire();\n\
         \tint Damage;\n\
         };\n",
    )
    .unwrap();
    fs::write(
        root.join("Source/Game/Weapon.cpp"),
        "#include \"Game/Weapon.h\"\n\
         void UWeapon::Fire()\n\
         {\n\
         \tApplyDamage();\n\
         }\n",
    )
    .unwrap();
    fs::create_dir_all(root.join(".alexandria/knowledge/modules")).unwrap();
    fs::write(
        root.join(".alexandria/alexandria.toml"),
        "[scan]\n\
         include_dirs = [\"Source\"]\n\
         [index]\n\
         docs_dirs = [\".alexandria/knowledge\"]\n\
         enabled_packs = [\"test-pack\"]\n",
    )
    .unwrap();
    fs::write(
        root.join(".alexandria/knowledge/modules/Weapon.md"),
        "---\nmodule: Game/Weapon\n---\n\n\
         # Weapon Module\n\n\
         The Weapon module owns the weapon runtime object and its firing behaviour\n\
         for the test project.\n\n\
         ## Key Claims\n\n\
         - [extracted] `UWeapon` is defined at `Source/Game/Weapon.h:2` and is the weapon root class.\n\
         - [inferred] Damage flows from the Fire entry point into the damage pipeline.\n\n\
         ## Boundaries\n\n\
         - The Weapon module does **not** cover ammunition or reload behaviour.\n\n\
         ## Evidence\n\n\
         - `UWeapon` defined at `Source/Game/Weapon.h:2`\n",
    )
    .unwrap();
    // A shared pack whose knowledge binds to the project's code late.
    fs::create_dir_all(root.join(".alexandria/packs/test-pack/domains")).unwrap();
    fs::write(
        root.join(".alexandria/packs/test-pack/domains/Combat.md"),
        "---\ndomain: Combat\n---\n\n\
         # Combat\n\n\
         The Combat domain strings weapons and damage into the end-to-end flow\n\
         across the test project's modules.\n\n\
         ## Key Claims\n\n\
         - [inferred] `UWeapon` participates in the combat damage flow from fire to impact.\n\n\
         ## Boundaries\n\n\
         - The Combat domain does **not** cover melee fighting of any kind.\n\n\
         ## Evidence\n\n\
         - `UWeapon` defined at `Source/Game/Weapon.h:2`\n",
    )
    .unwrap();
}

fn build_libraries(project: &Path) {
    let (ok, _, err) = library(project, &["scan"]);
    assert!(ok, "scan failed: {err}");
    let (ok, _, err) = library(project, &["compile"]);
    assert!(ok, "compile failed: {err}");
    let pack = project.join(".alexandria/packs/test-pack");
    let (ok, _, err) = library(
        project,
        &["compile", "--pack", pack.to_str().unwrap()],
    );
    assert!(ok, "compile --pack failed: {err}");
}

#[test]
fn free_function_call_to_ambiguous_name_stays_unresolved() {
    // A free (non-member) caller has no class scope; a callee name with
    // several free-function definitions elsewhere must NOT resolve to an
    // arbitrary one. Observable via callees: an unresolved edge keeps the
    // caller's file, a resolved one points at the definition file.
    let project = temp_project("freescope");
    fs::create_dir_all(project.join("Source/Game")).unwrap();
    fs::write(
        project.join("Source/Game/A.cpp"),
        "void Main()\n{\n\tHelper();\n}\n",
    )
    .unwrap();
    fs::write(project.join("Source/Game/B.cpp"), "void Helper() {}\n").unwrap();
    fs::write(project.join("Source/Game/C.cpp"), "void Helper() {}\n").unwrap();
    fs::create_dir_all(project.join(".alexandria/knowledge")).unwrap();
    fs::write(
        project.join(".alexandria/alexandria.toml"),
        "[scan]\ninclude_dirs = [\"Source\"]\n[index]\ndocs_dirs = [\".alexandria/knowledge\"]\n",
    )
    .unwrap();
    let (ok, _, err) = library(&project, &["scan"]);
    assert!(ok, "scan failed: {err}");

    let graph = json_stdout(&project, &["graph", "callees", "Main", "--json"]);
    let nodes = graph["nodes"].as_array().unwrap();
    let helper = nodes
        .iter()
        .find(|n| n["label"].as_str().unwrap().contains("Helper"))
        .expect("a Main → Helper edge expected");
    assert_eq!(
        helper["file"].as_str().unwrap(),
        "Source/Game/A.cpp",
        "ambiguous free-function call must stay at the call site, got {nodes:?}"
    );

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn references_resolve_cross_file_class_scope() {
    // The field is declared in the header, written from the .cpp: class-scope
    // resolution must find the write site, and locate must prefer the
    // header's definition.
    let project = temp_project("resolve");
    fs::create_dir_all(project.join("Source/Game")).unwrap();
    fs::write(
        project.join("Source/Game/Weapon.h"),
        "#pragma once\n\
         class UWeapon\n\
         {\n\
         public:\n\
         \tvoid Fire();\n\
         \tint Damage;\n\
         };\n",
    )
    .unwrap();
    fs::write(
        project.join("Source/Game/Weapon.cpp"),
        "#include \"Game/Weapon.h\"\n\
         void UWeapon::Fire()\n\
         {\n\
         \tDamage = 10;\n\
         \tDamage += 1;\n\
         }\n",
    )
    .unwrap();
    fs::create_dir_all(project.join(".alexandria/knowledge")).unwrap();
    fs::write(
        project.join(".alexandria/alexandria.toml"),
        "[scan]\ninclude_dirs = [\"Source\"]\n[index]\ndocs_dirs = [\".alexandria/knowledge\"]\n",
    )
    .unwrap();
    let (ok, _, err) = library(&project, &["scan"]);
    assert!(ok, "scan failed: {err}");

    // locate: the field resolves to its header declaration site.
    let located = json_stdout(&project, &["locate", "Damage", "--json"]);
    let first = &located.as_array().unwrap()[0];
    assert_eq!(first["kind"].as_str().unwrap(), "field");
    assert_eq!(first["file"].as_str().unwrap(), "Source/Game/Weapon.h");
    assert_eq!(first["qualified_name"].as_str().unwrap(), "UWeapon::Damage");

    // references: both write sites in the .cpp are found, attributed to Fire.
    let graph = json_stdout(&project, &["graph", "references", "Damage", "--json"]);
    let nodes = graph["nodes"].as_array().unwrap();
    let writes: Vec<&Value> = nodes
        .iter()
        .filter(|n| n["relation"].as_str() == Some("writes"))
        .collect();
    assert_eq!(writes.len(), 2, "expected two write sites, got {nodes:?}");
    assert!(
        writes
            .iter()
            .all(|n| n["file"].as_str() == Some("Source/Game/Weapon.cpp")
                && n["label"].as_str().unwrap().starts_with("Fire"))
    );

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn full_pipeline_scan_compile_query_locate() {
    let project = temp_project("pipeline");
    seed_project(&project);
    build_libraries(&project);

    // query finds the module document (BM25 route) and the file node.
    let hits = json_stdout(&project, &["query", "weapon fire damage", "--brief", "--json"]);
    let titles: Vec<&str> = hits
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|hit| hit["title"].as_str())
        .collect();
    assert!(
        titles.iter().any(|t| t.contains("Weapon")),
        "expected a Weapon hit, got {titles:?}"
    );

    // locate: the class resolves to the header definition.
    let located = json_stdout(&project, &["locate", "UWeapon", "--json"]);
    let first = &located.as_array().unwrap()[0];
    assert_eq!(first["file"].as_str().unwrap(), "Source/Game/Weapon.h");

    // locate Fire: the definition in the .cpp wins over the declaration in
    // the .h (role='definition' ordering), with the qualified name recorded.
    let located = json_stdout(&project, &["locate", "Fire", "--json"]);
    let first = &located.as_array().unwrap()[0];
    assert_eq!(first["role"].as_str().unwrap(), "definition");
    assert_eq!(first["file"].as_str().unwrap(), "Source/Game/Weapon.cpp");
    assert_eq!(first["qualified_name"].as_str().unwrap(), "UWeapon::Fire");

    // claim verification: the extracted claim verifies against real code.
    let packet = json_stdout(&project, &["query", "weapon root class", "--json"]);
    let verified = packet.as_array().unwrap()[0]["answerability"]["verified_claims"]
        .as_u64()
        .unwrap();
    assert!(verified >= 1, "expected >=1 verified claim, got {verified}");

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn pack_late_binding_and_refs_fanout() {
    let project = temp_project("packs");
    seed_project(&project);
    build_libraries(&project);

    // refs fans out across libraries; the pack row binds late to the project's
    // code index (it was stored unresolved in pack.db).
    let refs = json_stdout(&project, &["refs", "UWeapon", "--json"]);
    let rows = refs.as_array().unwrap();
    let pack_row = rows
        .iter()
        .find(|row| row["library"].as_str() == Some("test-pack"))
        .expect("expected a test-pack ref row");
    assert_eq!(
        pack_row["resolved_file"].as_str(),
        Some("Source/Game/Weapon.h"),
        "pack ref must late-bind to the project code index"
    );
    let project_row = rows
        .iter()
        .find(|row| row["library"].as_str() == Some("project"))
        .expect("expected a project ref row");
    assert_eq!(project_row["resolved_file"].as_str(), Some("Source/Game/Weapon.h"));

    // contract passes on both libraries (documents follow the spec).
    let contract = json_stdout(&project, &["contract", "--json"]);
    let libraries = contract["libraries"].as_array().expect("aggregated libraries");
    assert_eq!(libraries.len(), 2, "project + test-pack libraries expected");
    for library_report in libraries {
        assert_eq!(library_report["quarantined"].as_u64(), Some(0));
    }

    // lint is clean and exits zero.
    let (ok, _out, err) = library(&project, &["lint"]);
    assert!(ok, "lint failed: {err}");

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn feedback_loop_warns_until_cleared() {
    let project = temp_project("feedback");
    seed_project(&project);
    build_libraries(&project);

    // Realistic agent flow: take the node address straight from a query hit.
    let hits = json_stdout(&project, &["query", "weapon module owns runtime", "--brief", "--json"]);
    let hit = hits
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["title"].as_str() == Some("Weapon Module"))
        .expect("Weapon Module hit expected");
    let node_id = hit["node_id"].as_str().unwrap().to_string();
    let hit_library = hit["library"].as_str().unwrap().to_string();

    // The agent records a 'wrong' verdict for the module document.
    let (ok, _, err) = library(
        &project,
        &[
            "feedback",
            "wrong",
            "--query",
            "weapon fire damage",
            "--node",
            &node_id,
            "--library",
            &hit_library,
            "--note",
            "integration test says it is wrong",
        ],
    );
    assert!(ok, "feedback record failed: {err}");

    // The next packet for that unit carries the warning (check every packet —
    // fusion order decides which hit hosts it).
    fn all_warnings(packets: &Value) -> Vec<String> {
        packets
            .as_array()
            .map(|all| {
                all.iter()
                    .flat_map(|p| {
                        p["warnings"]
                            .as_array()
                            .map(|w| {
                                w.iter()
                                    .filter_map(|x| x.as_str().map(String::from))
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default()
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    let packets = json_stdout(&project, &["query", "weapon module", "--json"]);
    let warnings = all_warnings(&packets);
    assert!(
        warnings.iter().any(|w| w.contains("marked 'wrong'")),
        "expected a feedback warning, got {warnings:?}"
    );

    // Clearing removes the warning.
    let (ok, _, err) = library(&project, &["feedback", "--clear", &node_id]);
    assert!(ok, "feedback clear failed: {err}");
    let packets = json_stdout(&project, &["query", "weapon module", "--json"]);
    let warnings = all_warnings(&packets);
    assert!(
        !warnings.iter().any(|w| w.contains("marked 'wrong'")),
        "warning should be gone after clear: {warnings:?}"
    );

    let _ = fs::remove_dir_all(&project);
}
