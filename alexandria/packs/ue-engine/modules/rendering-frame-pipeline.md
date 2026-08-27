---
name: rendering-frame-pipeline
description: The rendering frame — from scene and primitive proxies through visibility, mesh passes, and the RDG-built frame graph to the RHI — the map for finding where any visual feature lives.
module: rendering-frame-pipeline
---

# Rendering Frame Pipeline (UE5 Expert)

## Context
- Read before adding any rendering feature, profiling GPU time, or wondering why a component change "doesn't show up".
- The pack's rendering domain covers RDG and shader authoring in depth; this doc is the frame those passes plug into.

## 1. The Frame at a Glance

```
Game thread: scene state updates (transforms, visibility toggles)
        │
        ▼
Render thread: FScene — proxy mirror of all primitives/lights
        │
        ▼
FSceneRenderer: InitViews — frustum + occlusion culling → visible mesh commands
        │
        ▼
FRDGBuilder: passes declared (shadows → base pass → lighting → post)
        │
        ▼
RHI: GPU command stream
```

The scene (`FScene`) is the renderer's world; game-thread components never touch GPU state directly — they enqueue proxy updates that apply on the render thread.

## 2. The Proxy Split (where most bugs live)

Every renderable thing has a game-thread component and a render-thread proxy (`FPrimitiveSceneProxy`). Changing a UPROPERTY does nothing until a proxy update is enqueued — marking the component dirty/re-creating the proxy is the required half of any visual edit. Async work touching proxies or the scene from the wrong thread is a crash that looks random.

## 3. Visibility and Mesh Passes

`InitViews` culls per view (frustum, distance, occlusion) and produces mesh commands sorted into passes. A material's blend mode and settings decide which passes an object enters — "object invisible" is often a pass-membership problem (e.g. translucent after depth-only), not a rendering bug.

## 4. RDG's Role

`FRDGBuilder` records passes and resources for the whole frame, then schedules them with automatic barriers, transient allocation, and async compute lanes. Passes declare inputs/outputs; the graph decides execution order — writing passes that reach around the graph (raw UAV writes without parameters) breaks the scheduling contract.

## Verification Checklist

- Visual change doesn't appear: proxy dirty flag/re-create missing — the scene still has the old state.
- GPU time mystery: `r.RDG` graph dumps + `ProfileGPU` — find the pass, then the mesh commands inside it.
- Flicker/race after async change: you touched render-thread state from a game-thread task.

## Architecture

The pipeline is a strict two-world model: game world mutates, render world mirrors through proxies, and a declarative frame graph consumes the mirror. Every modern engine headache (thread races, state divergence, barrier bugs) comes from poking holes in one of those three walls.

## Data Flow

```
USceneComponent/UPrimitiveComponent (game thread)
        │ enqueue proxy updates
        ▼
FScene + FPrimitiveSceneProxy (render thread)
        │
        ▼
InitViews → culling → mesh commands per pass
        │
        ▼
FRDGBuilder records passes/resources → schedule
        │
        ▼
RHI → GPU
```

## Key Claims

- [extracted] `FSceneRenderer` is defined at `Source/Runtime/Renderer/Private/SceneRendering.h` and drives per-view visibility and pass setup.
- [extracted] `FRDGBuilder` is defined at `Source/Runtime/RenderCore/Public/RenderGraphBuilder.h` and records and schedules the frame's passes and resources.
- [inferred] The component/proxy split means every visual change is a two-step: mutate game state, then enqueue the proxy update that mirrors it.

## Edge Cases

- Scene capture views run the same pipeline with their own view state — effects reading global view uniforms break in captures.
- Editor-only primitives (selection outlines, gizmos) live in separate passes and do not exist in packaged builds.
- Nanite changes pass membership wholesale (cluster culling) — pass-level assumptions from the mesh era do not transfer.

## Boundaries

- The frame pipeline stops at pass scheduling; shader authoring and RDG pass writing are covered by the rendering domain docs.
- Slate/UI rendering runs on a separate path that composites at the very end.

## Evidence

- `FSceneRendererBase` defined at `Source/Runtime/Renderer/Private/SceneRendering.h`
- `FSceneRenderer` defined at `Source/Runtime/Renderer/Private/SceneRendering.h`
- `FRDGBuilder` defined at `Source/Runtime/RenderCore/Public/RenderGraphBuilder.h`
