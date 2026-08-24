---
name: fab-marketplace-submission-requirements
description: Structuring plugins for Fab Marketplace submission. Rules for folder naming, versioning, and passing automated validation.
feature: fab-marketplace-submission-requirements
module: Developer
---

# Fab Marketplace Submission Requirements (UE5 Expert)

## Context
- Mandatory when preparing a plugin, asset pack, or tool for commercial release on the Fab Marketplace (Epic's unified store).
- Trigger at the start of plugin creation to ensure compliance from day one.

## 1. Directory & Naming Strictness
Fab's automated ingestion systems are incredibly strict.

- **No Spaces or Special Characters**: Folder and file names must use `A-Z`, `a-z`, `0-9`, and `_`. No spaces, no hyphens `-`.
- **Top Level Folder**: Your plugin must reside in a single root folder matching the exact Plugin Name.
- **Filter Paths**: Ensure no `Saved/`, `Intermediate/`, or `Binaries/` folders are included in your submission zip.

## 2. The .uplugin Configuration
The `.uplugin` file is your manifest. It MUST be fully populated.

```json
{
    "FileVersion": 3,
    "Version": 1,
    "VersionName": "1.0",
    "FriendlyName": "My Advanced Agent Tool",
    "Description": "A tool that automates XYZ.",
    "Category": "Editor",
    "CreatedBy": "Your Studio Name",
    "CreatedByURL": "https://yourstudio.com",
    "DocsURL": "https://yourstudio.com/docs",
    "MarketplaceURL": "", // Provided by Epic later
    "SupportURL": "mailto:support@yourstudio.com",
    "EngineVersion": "5.4.0",
    "CanContainContent": true,
    "IsBetaVersion": false,
    "IsExperimentalVersion": false,
    "Installed": false
}
```

## 3. C++ Code Requirements
- **No Absolute Paths**: Code must never reference `C:/...`. Use `FPaths` or `IPluginManager`.
- **No Engine Modifications**: You cannot distribute modified Engine source files.
- **Third-Party Libraries**: Any `.dll` or `.lib` must be legally redistributable, placed in a `ThirdParty/` folder, and statically linked via `Build.cs`.


## Verification Checklist
- [ ] No spaces in any folder or filename.
- [ ] `.uplugin` file contains valid URLs, Version, and EngineVersion.
- [ ] `Saved`, `Intermediate`, and `Binaries` folders deleted before zipping.
- [ ] All C++ classes and structs use correct Prefixes (A, U, F) to pass UHT validation.
- [ ] Code compiles cleanly on Development Editor and Shipping configurations.

## Architecture

The architecture for Fab Marketplace Submission Requirements follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Fab Marketplace Submission Requirements.

## Key Claims

- **Rejection Prevention**: Failing these checks results in automatic rejection by Fab bots, delaying release by days or weeks.
- **Cross-Project Safety**: A correctly namespaced plugin ensures it will not collide with other plugins the user has installed.


The Fab Marketplace Submission Requirements pattern follows UE5 engine conventions and best practices.

## Edge Cases

Refer to the Common Mistakes section for Fab Marketplace Submission Requirements edge cases. Ensure proper null checks and UPROPERTY replication for networked usage.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h:1`

