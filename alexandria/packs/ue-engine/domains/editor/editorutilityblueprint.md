---
name: editorutilityblueprint
description: Developing EditorUtilityBlueprints for logic-only editor workflows. Enables fast iteration without C++ recompilation.
domain: editor
---

# EditorUtilityBlueprint (UE5 Expert)

## Context
- Use for rapid prototyping of Editor tools.
- Trigger when designers need custom scripted actions without touching C++.
- Ideal for Level manipulation, asset generation, or validation scripts.

## 1. Core Principles
- **EditorUtilityBlueprint (EUB)**: Acts like a standard Blueprint but has access to Editor-only nodes (e.g., `GetSelectedActors`, `SpawnActorInEditor`).
- **No Recompile Required**: Logic changes apply instantly upon compile within the Editor.

## 2. Creating Scripted Actions

### Asset Actions
Create an EUB inheriting from `AssetActionUtility`.
- Functions marked "Call In Editor" appear when right-clicking assets.

### Actor Actions
Create an EUB inheriting from `ActorActionUtility`.
- Functions marked "Call In Editor" appear when right-clicking Actors in the Level/Outliner.

## 3. Bridging with C++
EUBs are powerful but slow for complex math. 
- **Rule**: Write heavy loops or algorithms in a C++ `UBlueprintFunctionLibrary`, then call them from the EUB.


## Edge Cases
**BAD (Heavy Logic in EUB)**:
```text
Executing a 10,000-iteration loop in a Blueprint graph to process vertices. Will freeze the Editor.
```

**GOOD (Hybrid Approach)**:
```text
EUB calls a C++ UBlueprintFunctionLibrary node `ProcessVertices(MyMesh)`. C++ does the heavy lifting instantly.
```

## Verification Checklist
- [ ] Ensure EUBs are saved in an Editor-only or Developer folder.
- [ ] Heavy iterative logic is offloaded to C++ static libraries.
- [ ] Correct parent class used (`AssetActionUtility` vs `ActorActionUtility` vs `EditorUtilityWidget`).

## Architecture

The architecture for Editorutilityblueprint follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Editorutilityblueprint.

## Key Claims

- **Non-Destructive**: EUBs only run in the Editor. They cannot break the packaged runtime build.
- **Rapid Iteration**: Agents can generate an EUB layout, and humans can test it instantly without waiting for a Live Coding compilation.


The Editorutilityblueprint pattern follows UE5 engine conventions and best practices.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h`

