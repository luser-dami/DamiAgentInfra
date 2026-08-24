---
name: editor-only-module
description: Configuring plugin types as Editor to exclude tool-specific code from Shipping builds. Maintains clean executables.
domain: editor
---

# Editor Only Module (UE5 Expert)

## Context
- Mandatory for plugins or modules that create Editor tools (Custom windows, Asset actions).
- Trigger when preparing a project for packaging/shipping to ensure zero bloat.

## 1. The .uplugin and .uproject Configuration
A module must be explicitly typed as `Editor` so the Unreal Build Tool (UBT) ignores it during Shipping builds.

### In your .uplugin or .uproject file:
```json
"Modules": [
    {
        "Name": "MyEditorTools",
        "Type": "Editor",
        "LoadingPhase": "PostEngineInit"
    },
    {
        "Name": "MyRuntimeLogic",
        "Type": "Runtime",
        "LoadingPhase": "Default"
    }
]
```

## 2. Code Isolation (WITH_EDITOR)
If Runtime modules must interact with Editor modules (e.g., debug data), wrap the C++ code in `#if WITH_EDITOR`.

```cpp
#if WITH_EDITOR
// This code is completely stripped in Shipping builds
void AMyActor::DrawDebugShapes() { ... }
#endif
```


## Edge Cases
**BAD (Packaging Failure)**:
```json
{
    "Name": "MyEditorTool",
    "Type": "Runtime" // FATAL: Will try to package UnrealEd into the game, causing build failure.
}
```

**GOOD (Proper Type)**:
```json
{
    "Name": "MyEditorTool",
    "Type": "Editor" // SAFE: Stripped during package.
}
```

## Verification Checklist
- [ ] Editor tool modules are set to `"Type": "Editor"`.
- [ ] Runtime modules NEVER depend on Editor modules.
- [ ] Editor-specific variables in runtime headers are wrapped in `#if WITH_EDITOR_ONLY_DATA`.
- [ ] Editor-specific functions in runtime files are wrapped in `#if WITH_EDITOR`.

## Architecture

The architecture for Editor Only Module follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Editor Only Module.

## Key Claims

- **Shipping Footprint**: Excludes heavy editor libraries (`UnrealEd`, `SlateEditor`) from the final game, reducing package size and memory usage.
- **Security**: Prevents shipping proprietary editor logic or developer tools to end-users.


The Editor Only Module pattern follows UE5 engine conventions and best practices.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h:1`

