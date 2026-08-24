---
name: hlsl-custom-expressions
description: Writing raw HLSL shader code within the Material Editor via Custom Nodes. Handles complex mathematical effects safely.
domain: rendering
---

# HLSL Custom Expressions (UE5 Expert)

## Context
- Use when an effect requires loops (`for`, `while`), complex math (Raymarching, Fractals), or custom texture sampling logic that is inefficient in visual nodes.
- Trigger when standard material graphs become impossibly large or difficult to maintain.

## 1. Core Principles
The `Custom` node in the Material Editor allows raw HLSL code. In UE5, this code is injected directly into the generated shader during compilation.

## 2. Best Practices for HLSL in UE5

### 1. The Virtual File Pattern
Instead of writing 500 lines of HLSL directly in the tiny Custom Node text box, create a `.usf` (Unreal Shader File) or `.ush` file in your project's `Shaders/` folder.

**In the Custom Node:**
```hlsl
#include "/Project/MyShaders/ComplexMath.ush"
return CalculateFractal(UV, Iterations);
```

*Note: Requires enabling "Allow Project Shaders" in Project Settings.*

### 2. Handling Inputs
Define exact inputs in the Custom Node properties. If you define `float3 A` and `float B`, your code can use them directly.

```hlsl
// Example: Simple Custom Node Code
float result = 0;
for(int i=0; i < Iterations; i++) {
    result += A.x * B;
}
return result;
```


## Edge Cases
**BAD (Simple Math in HLSL)**:
```hlsl
return A * B + C; // BAD: Defeats the material compiler's ability to optimize. Use Add/Multiply nodes.
```

**GOOD (Required HLSL)**:
```hlsl
// Raymarching loop (Impossible with standard nodes)
float dist = 0;
for(int i=0; i<64; i++) {
    dist += GetSceneDistance(Pos + Dir * dist);
}
return dist;
```

## Verification Checklist
- [ ] HLSL code does not replicate logic easily done with standard nodes.
- [ ] Code is stored in `.ush` files for complex algorithms to allow IDE syntax highlighting.
- [ ] Inputs and Output Types are strictly defined in the node properties.

## Architecture

The architecture for Hlsl Custom Expressions follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Hlsl Custom Expressions.

## Key Claims

- **Cross-Platform Compilation**: HLSL written in Custom Nodes must compile to GLSL (Vulkan), Metal, and console shading languages via Unreal's Shader Cross-Compiler. Avoid highly platform-specific intrinsic functions.
- **Optimization Restrictions**: The UE Material Compiler cannot heavily optimize or fold code inside a Custom Node. Only use them when absolutely necessary; visual nodes are generally faster for simple math due to aggressive compiler folding.


The Hlsl Custom Expressions pattern follows UE5 engine conventions and best practices.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h:1`

