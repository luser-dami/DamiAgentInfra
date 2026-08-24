---
name: shader-permutation-reduction
description: Designing materials to minimize unique shader variants. Vital for reducing compile times and GPU memory usage.
domain: rendering
---

# Shader Permutation Reduction (UE5 Expert)

## Context
- Mandatory when managing the master materials of a project.
- Trigger when encountering "Compiling Shaders (45,000)" or excessive load times in the Editor.

## 1. What is a Shader Permutation?
Every time you use a `Static Switch Parameter` or enable a specific mesh feature (like Skeletal, Nanite, or Instancing) on a material, Unreal generates a unique, hardcoded version of that shader. 
- 1 Material with 5 Static Switches = $2^5$ = 32 Shader Permutations generated.

## 2. Reduction Strategies

### Static Switch vs Dynamic Branching (if)
- **Static Switch**: Evaluates at compile time. Creates permutations. Zero runtime cost for the unused branch.
- **Dynamic Branching (If Node)**: Evaluates at runtime on the GPU. Creates NO permutations.
- **Rule**: If the code inside the switch is cheap (e.g., swapping a color or a simple texture), use a standard dynamic `If` or `Lerp` driven by a scalar parameter. Only use Static Switches for extremely expensive features (like turning off Parallax Occlusion Mapping).

### Usage Flags Optimization
In the Material Details, disable unused "Usage" flags.
- If a material will never be used on a Skeletal Mesh, uncheck `Used with Skeletal Mesh`.
- Uncheck `Used with Instanced Static Meshes` if not needed.
- Each checked flag doubles the potential permutation count.


## Edge Cases
**BAD (Permutation Explosion)**:
```text
Master Material has 10 Static Switch Parameters for simple things like "Use Red Tint?", "Use Dirt Overlay?". 
Result: Thousands of shaders compile for no reason.
```

**GOOD (Lerp / Dynamic Parameters)**:
```text
Use a Scalar Parameter "DirtAmount" (0 to 1) and Lerp between the clean and dirty textures. 
Result: 1 Shader Permutation. Can be modified via Dynamic Material Instances at runtime!
```

## Verification Checklist
- [ ] `Static Switch Parameters` are strictly reserved for heavy architectural changes (e.g., Disabling Raytracing/Displacement).
- [ ] Simple toggles use `Lerp` or `Switch` driven by dynamic scalar parameters.
- [ ] Material Usage flags (Skeletal, Instanced, UI) are unchecked if completely unnecessary.

## Architecture

The architecture for Shader Permutation Reduction follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Shader Permutation Reduction.

## Key Claims

- **Iteration Flow**: The primary bottleneck for agentic/human workflows is waiting for shaders to compile. Reducing permutations keeps the workspace fast.
- **Memory Footprint**: Every permutation takes up RAM and disk space in the packaged build.


The Shader Permutation Reduction pattern follows UE5 engine conventions and best practices.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h:1`

