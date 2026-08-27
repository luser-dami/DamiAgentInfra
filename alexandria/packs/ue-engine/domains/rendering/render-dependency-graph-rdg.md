---
name: render-dependency-graph-rdg
description: Basic Render Dependency Graph (RDG) setup. Creating rendering passes with FRDGBuilder.
domain: rendering
---

# Render Dependency Graph (RDG) (UE5 Expert)

## Context
- Mandatory when executing ANY custom rendering commands in UE5.
- Trigger when injecting post-process passes, dispatching compute shaders, or drawing custom geometry.

## 1. Core Principles
RDG defers the execution of rendering commands. Instead of sending commands immediately to the GPU, you build a "Graph" of passes. 
- RDG analyzes the graph to optimize memory allocation (aliasing overlapping buffers) and barrier synchronization automatically.

## 2. Implementation Syntax

### 1. The Render Command Enqueue
Rendering code MUST run on the Render Thread.

```cpp
ENQUEUE_RENDER_COMMAND(MyCustomPassCommand)(
    [this](FRHICommandListImmediate& RHICmdList)
    {
        // Initialize the Graph Builder
        FRDGBuilder GraphBuilder(RHICmdList);

        // ... Build your graph here ...

        // Execute the graph (Compiles and runs on GPU)
        GraphBuilder.Execute();
    });
```

### 2. Creating RDG Resources
You do not create raw RHI textures. You create RDG textures.

```cpp
FRDGTextureDesc Desc = FRDGTextureDesc::Create2D(
    FIntPoint(1920, 1080), 
    PF_FloatRGBA, 
    FClearValueBinding::Black, 
    TexCreate_RenderTargetable | TexCreate_ShaderResource
);

FRDGTextureRef MyRDGTexture = GraphBuilder.CreateTexture(Desc, TEXT("MyCustomTexture"));
```

### 3. Adding a Pass
Passes require a name, parameters, and a lambda containing the execution logic.

```cpp
FMyShaderParameters* PassParameters = GraphBuilder.AllocParameters<FMyShaderParameters>();
PassParameters->OutputTexture = MyRDGTexture;

// ERDGPassFlags::Raster if drawing geometry/post-process
// ERDGPassFlags::Compute if dispatching a compute shader
GraphBuilder.AddPass(
    RDG_EVENT_NAME("My Custom Rendering Pass"),
    PassParameters,
    ERDGPassFlags::Raster,
    [PassParameters](FRHICommandList& RHICmdList)
    {
        // Low-level RHI drawing commands go here
        // Set Pipeline State, DrawPrimitive, etc.
    });
```


## Edge Cases
**BAD (Raw RHI allocation)**:
```cpp
FTexture2DRHIRef RawTexture;
RHICreateTexture2D(..., RawTexture); // BAD: Bypasses RDG memory optimizations.
```

**GOOD (RDG allocation)**:
```cpp
FRDGTextureRef RDGTexture = GraphBuilder.CreateTexture(Desc, TEXT("MyTex")); // SAFE
```

## Verification Checklist
- [ ] All code using `FRDGBuilder` executes within `ENQUEUE_RENDER_COMMAND` or an existing Render Thread callback.
- [ ] Textures and buffers are created via `GraphBuilder` (RDG resources), not raw RHI.
- [ ] `GraphBuilder.Execute()` is called at the end of the ENQUEUE block.
- [ ] `RDG_EVENT_NAME` is provided for profiling in RenderDoc / Unreal Insights.

## Architecture

The architecture for Render Dependency Graph Rdg follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Render Dependency Graph Rdg.

## Key Claims

- **GPU Synchronization**: RDG completely eliminates the need for manual resource barriers (e.g., transitioning a texture from Write to Read). RDG calculates this via the `PassParameters` struct.
- **Memory Savings**: RDG aggressively recycles transient memory. Temporary buffers created via `GraphBuilder.CreateTexture` consume 0 VRAM if their lifetime doesn't overlap with other passes.


The Render Dependency Graph Rdg pattern follows UE5 engine conventions and best practices.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h`

