---
name: slate-umg-architecture
description: Slate and UMG architecture — SWidget's retained tree, invalidation-driven painting, and how UUserWidget wraps Slate for designers — plus the performance rules that fall out.
module: slate-umg-architecture
---

# Slate & UMG Architecture (UE5 Expert)

## Context
- Read before writing custom widgets, profiling UI hitches, or binding game state to UMG.
- Pack feature docs cover Slate syntax and UMG lifecycle; this doc is the architecture those patterns sit on.

## 1. Two Layers, One Tree

| Layer | Class | Role |
|-------|-------|------|
| Slate | `SWidget` tree (`FSlateApplication`) | Retained UI: layout, input, painting |
| UMG | `UUserWidget` (UWidget children) | Designer-facing wrapper that builds and owns a Slate subtree |

UMG is not a separate UI system — a UUserWidget constructs Slate widgets (or wraps custom ones) and forwards property changes to them.

## 2. Retained, Not Immediate

Slate keeps a widget tree with its own layout and paint state. Each frame it does **only** what's invalidated: layout pass for dirty geometry, paint pass for dirty visuals. The performance contract:

- Bind-free const widgets cost almost nothing after creation.
- Every per-frame property change (text, progress, visibility) invalidates and re-runs the affected subtree — hundreds of bindings firing per frame is the classic UMG hitch.

## 3. Painting and the Draw Path

`SWidget::Paint` emits draw elements into a window draw list; `FSlateApplication` batches and hands them to the RHI at frame end, compositing over the 3D scene. Custom drawing = override Paint/OnPaint and emit elements — everything else (materials, transforms) flows through those elements.

## 4. UMG Binding Rules That Follow

- Event delegates (OnClicked) are cheap; property **bindings** poll or push per frame — prefer event-driven updates from game state changes.
- Heavy widgets (list views with many entries) virtualize; constructing entries up front is a creation-time spike.
- Widget components in 3D space pay full Slate cost per world-space widget.

## Verification Checklist

- UI hitch: `Slate.Benchmark` / widget reflector — find the invalidated subtree, then the binding driving it.
- Binding not updating: the wrapped Slate widget needs its own setter called — UMG property changes don't magically reach custom Slate.
- Input eaten: hit-test visibility chain — an invisible-but-hit-testable parent is swallowing clicks.

## Architecture

Slate is a retained-mode tree optimized to do nothing: invalidation flags decide what recomputes each frame. UMG trades a thin abstraction tax for designer workflow, keeping the same invalidation model underneath. Treating UMG as immediate-mode (rebuild, rebind, re-create every frame) is the root of nearly all UI performance problems.

## Data Flow

```
Game state change
        │
        ▼
UUserWidget property / event
        │
        ▼
Slate widget setter → invalidation flag
        │
        ▼
FSlateApplication: layout pass (dirty geometry) → paint pass (dirty visuals)
        │
        ▼
Draw elements → batch → RHI composite over scene
```

## Key Claims

- [extracted] `SWidget` is defined at `Source/Runtime/SlateCore/Public/Widgets/SWidget.h` and is the base of the retained Slate tree.
- [extracted] `SCompoundWidget` is defined at `Source/Runtime/SlateCore/Public/Widgets/SCompoundWidget.h` and is the standard single-child custom widget base.
- [extracted] `UUserWidget` is defined at `Source/Runtime/UMG/Public/Blueprint/UserWidget.h` and wraps a Slate subtree for the UMG designer workflow.
- [extracted] `FSlateApplication` is defined at `Source/Runtime/Slate/Public/Framework/Application/SlateApplication.h` and owns layout, input routing, and draw batching for the tree.
- [inferred] UI cost scales with invalidations per frame, not widget count, so event-driven updates beat per-frame bindings.

## Edge Cases

- Ticks on widgets bypass the invalidation economy — a ticking UMG panel invalidates its subtree every frame by design.
- Editor Slate and game Slate share the tree but not the styles; editor extensions must not assume game styles exist.
- Invalidation inside Paint is forbidden — it re-enters the pass and produces one-frame-lag artifacts.

## Boundaries

- The architecture stops at the widget tree; MVVM viewmodels and CommonUI navigation are layers above with their own patterns.
- 3D scene rendering is the render pipeline's doc; Slate only composites at the end.

## Evidence

- `SWidget` defined at `Source/Runtime/SlateCore/Public/Widgets/SWidget.h`
- `SCompoundWidget` defined at `Source/Runtime/SlateCore/Public/Widgets/SCompoundWidget.h`
- `UUserWidget` defined at `Source/Runtime/UMG/Public/Blueprint/UserWidget.h`
- `FSlateApplication` defined at `Source/Runtime/Slate/Public/Framework/Application/SlateApplication.h`
