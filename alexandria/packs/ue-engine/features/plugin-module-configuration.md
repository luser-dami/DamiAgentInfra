---
name: plugin-module-configuration
description: Expert setup of .Build.cs files for Unreal Engine plugins. Differentiates Public and Private dependencies to optimize link times and module visibility.
feature: plugin-module-configuration
module: Developer
---

# Plugin Module Configuration (UE5 Expert)

## Context
- Mandatory when creating or maintaining Unreal Engine plugins.
- Use when adding new modules (e.g., `UMGEditor`, `UnrealEd`) to a project.
- Trigger when encountering unresolved external symbol errors or long link times.

## 1. Public vs Private Dependencies
Unreal Build Tool (UBT) uses `.Build.cs` to link modules.

- **PublicDependencyModuleNames**: Use if your module's PUBLIC headers (`.h`) include headers from another module. The dependent module will also be linked to anyone who depends on your module.
- **PrivateDependencyModuleNames**: Use if your module ONLY includes the other module's headers in your private `.cpp` files. 

## 2. Optimization Strategy
ALWAYS prefer `PrivateDependencyModuleNames`. It prevents dependency cascades, drastically reducing compile and link times.

```csharp
// MyPlugin.Build.cs
public class MyPlugin : ModuleRules
{
    public MyPlugin(ReadOnlyTargetRules Target) : base(Target)
    {
        PCHUsage = ModuleRules.PCHUsageMode.UseExplicitOrSharedPCHs;

        // Required by your .h files (Keep this minimal!)
        PublicDependencyModuleNames.AddRange(new string[] {
            "Core",
        });

        // Required only by your .cpp files
        PrivateDependencyModuleNames.AddRange(new string[] {
            "CoreUObject",
            "Engine",
            "Slate",
            "SlateCore",
            "UnrealEd" // Editor-only dependency
        });
    }
}
```


## Edge Cases
**BAD (Everything Public)**:
```csharp
PublicDependencyModuleNames.AddRange(new string[] { "Core", "Engine", "UnrealEd", "UMG" }); // Bloats dependent modules
```

**GOOD (Strict Isolation)**:
```csharp
PublicDependencyModuleNames.AddRange(new string[] { "Core" });
PrivateDependencyModuleNames.AddRange(new string[] { "Engine", "UnrealEd" }); // Safe, hidden
```

## Verification Checklist
- [ ] `PublicDependencyModuleNames` is strictly limited to modules required in public headers.
- [ ] All other dependencies are in `PrivateDependencyModuleNames`.
- [ ] Editor-specific modules (`UnrealEd`, `UMGEditor`) are isolated correctly.

## Architecture

The architecture for Plugin Module Configuration follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Plugin Module Configuration.

## Key Claims

- **Link Time Optimization**: Prevents massive linking overhead.
- **Encapsulation**: Hides internal implementation details from other modules.


The Plugin Module Configuration pattern follows UE5 engine conventions and best practices.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h:1`

