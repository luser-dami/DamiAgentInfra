---
name: navigation-pathfinding
description: The navigation stack — UNavigationSystemV1, Recast/Detour navmesh generation, and UPathFollowingComponent's move execution — and how AI actually gets from point A to B.
module: navigation-pathfinding
---

# Navigation & Pathfinding Internals (UE5 Expert)

## Context
- Read before debugging AI that won't move, paths through walls, or navmesh holes; and before choosing nav settings for a new map.
- BT tasks like MoveTo are thin wrappers — the machinery below decides everything.

## 1. The Three Layers

| Layer | Class | Owns |
|-------|-------|------|
| System | `UNavigationSystemV1` | Nav data registry, generation scheduling, queries |
| Data | `ARecastNavMesh` (Recast/Detour) | The actual walkable-surface mesh, chunked into tiles |
| Execution | `UPathFollowingComponent` | Steering an AI pawn along a found path |

A move request flows: AI controller → pathfinding query (Detour A* over navmesh polys) → path corridor → PathFollowingComponent walks segment by segment, steering with acceleration toward segment ends.

## 2. Generation: Where Navmesh Comes From

Recast rasterizes collision geometry into voxel spans → filters by agent properties (radius, height, max slope, step height) → builds a polygon mesh → tiles it. `NavMeshBoundsVolume` marks the space; recast actors regenerate tiles incrementally around changes (dynamic generation) or at build time (static). **Agent-size parameters are nav data, not AI code** — an agent that doesn't fit the navmesh config falls through floors the mesh never had.

## 3. Path Following Is Not the Path

The path is a poly corridor; `UPathFollowingComponent` turns it into movement input each tick (or drives the CMC directly). Blocked mid-corridor → repath or abort depending on move config. The "AI walks into walls" symptom is almost always: stale tile, agent radius mismatch, or a corridor that hugs geometry closer than the pawn's capsule.

## Verification Checklist

- AI doesn't move: `show Navigation` in PIE — is there mesh where the AI stands and where it goes?
- Holes in odd places: check agent Max Slope / Max Step Height vs the geometry.
- Works in editor, broken in packaged: navmesh was set to dynamic-only or bounds volumes were not packaged.
- Jittery movement: path following segment length and acceptance radius vs pawn acceleration.

## Architecture

Navigation is data-driven steering: the system produces walkable geometry once, queries are cheap graph searches over it, and execution is a dumb segment-follower fed by the CMC. Sophisticated behavior (avoidance, formation, reaction) belongs in the BT/perception layers above — pushing it into path following is fighting the design.

## Data Flow

```
BT MoveTo → AIController
        │
        ▼
UNavigationSystemV1 — find path (Detour A* over tiles)
        │
        ▼
Path corridor (poly list)
        │
        ▼
UPathFollowingComponent — per-tick steering into CMC acceleration
        │
        ▼
Repath on block / abort on unreachable
```

## Key Claims

- [extracted] `UNavigationSystemV1` is defined at `Source/Runtime/NavigationSystem/Public/NavigationSystem.h` and owns nav data registration and path queries.
- [extracted] `ARecastNavMesh` is defined at `Source/Runtime/NavigationSystem/Public/NavMesh/RecastNavMesh.h` and holds the generated Recast/Detour tile data.
- [extracted] `UPathFollowingComponent` is defined at `Source/Runtime/AIModule/Classes/Navigation/PathFollowingComponent.h` and converts a path corridor into per-tick pawn movement.
- [inferred] Most AI movement bugs are nav data configuration bugs (agent size, generation bounds, slope/step), not behavior-tree bugs.

## Edge Cases

- Multiple agent radii need multiple navmesh instances (supported nav agents), not one averaged mesh.
- Nav links (jump points, teleports) are annotations on top of the mesh — they do not affect generation, only query results.
- Moving platforms under static navmesh produce ghost paths; dynamic tile regeneration or nav-inviting actors are the fix.

## Boundaries

- The stack ends at corridor following; avoidance (RVO/detour crowd) and perception are separate systems with their own docs.
- Flying/swimming agents are not Recast's domain — they need custom or plugin navigation.

## Evidence

- `UNavigationSystemV1` defined at `Source/Runtime/NavigationSystem/Public/NavigationSystem.h`
- `ARecastNavMesh` defined at `Source/Runtime/NavigationSystem/Public/NavMesh/RecastNavMesh.h`
- `UPathFollowingComponent` defined at `Source/Runtime/AIModule/Classes/Navigation/PathFollowingComponent.h`
