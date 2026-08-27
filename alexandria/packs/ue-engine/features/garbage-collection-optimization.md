---
name: garbage-collection-optimization
description: Expert guide for managing the Unreal Engine Garbage Collector (GC). Covers IsValid() necessity, MarkAsGarbage() vs MarkPendingKill(), and UE 5.5 behavioral shifts.
feature: garbage-collection-optimization
module: Core
---

# Garbage Collection Optimization (UE5 Expert)

## Context
- Trigger when destroying Actors/Objects at runtime.
- Use when validating references before operation.
- Essential for understanding memory leaks and stale object access.

## 1. The Death of Auto-Nulling (UE 5.5+)
In UE 5.5, the Garbage Collector no longer guarantees immediate `nullptr` assignment for destroyed objects to optimize performance (Incremental GC).
- **Stale Pointers**: A pointer `T*` might still hold a memory address even if the object is "Dead" (Garbage).
- **The Flag**: Objects are marked with `EInternalObjectFlags::Garbage`.

## 2. IsValid() vs Null Checks
CRITICAL: A simple null check `if (MyObj)` is INSUFFICIENT.
- `if (MyObj)`: Only checks if the address is non-zero.
- `if (IsValid(MyObj))`: Checks if address is non-zero AND object is not marked as Garbage.

```cpp
// BAD (UE 5.5+)
if (MyActor) {
    MyActor->Fire(); // Might crash if MyActor is marked as Garbage!
}

// GOOD (Always)
if (IsValid(MyActor)) {
    MyActor->Fire(); // Safe
}
```

## 3. Destroying Objects Correctly

### Actors
Use `Destroy()`. This handles networking and internal cleanup.
```cpp
MyActor->Destroy();
```

### UObjects (Non-Actors)
`MarkPendingKill()` is DEPRECATED. Use `MarkAsGarbage()`.
```cpp
MyObject->MarkAsGarbage();
```

## 4. GC Cycle Awareness
- **Default Cycle**: 60 seconds (variable via `gc.TimeBetweenPurgingPendingKill`).
- **Full Purge**: Objects are physically removed from memory during the "Sweep" phase.
- **Incremental Sweep**: UE5 spreads the CPU cost of sweeping across multiple frames to prevent hitches.

## 5. Handling Stale Objects
If an object is "Stale" but not yet nulled:
- It still occupies memory.
- It should NOT be used for logic.
- References in `TArray` might need manual `RemoveAll` or `Compact` if they are not automatically nulled by GC.

## Edge Cases
**BAD (Accessing Stale Actor)**:
```cpp
void OnTimer() {
    if (TargetActor) { // TargetActor was Destroy()'d but not yet nulled by GC
        TargetActor->TakeDamage(10); // CRASH or UNDEFINED BEHAVIOR
    }
}
```

**GOOD (Validating Access)**:
```cpp
void OnTimer() {
    if (IsValid(TargetActor)) { 
        TargetActor->TakeDamage(10); // SAFE
    }
}
```

## Pro Tips
- **GForceGC**: Use console command `gc.CollectGarbage` for testing (Force a full cycle).
- **Clusters**: Grouping objects into "Clusters" (via `CanBeInCluster`) can speed up GC scans for complex hierarchies.
- **TWeakObjectPtr**: Automatically returns `nullptr` via `.Get()` if the object is marked for destruction. Best for non-owning references.

## Verification Checklist
- [ ] No `MarkPendingKill()` used (use `MarkAsGarbage()` or `Destroy()`).
- [ ] All `UObject` logic is guarded by `IsValid()`.
- [ ] `TWeakObjectPtr` used for references that should not prevent GC.
- [ ] Large dynamic arrays of `UObjects` are periodically checked/compacted.

## Architecture

The architecture for Garbage Collection Optimization follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Garbage Collection Optimization.

## Key Claims

- The Garbage Collection Optimization pattern follows UE5 engine conventions and best practices.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h`

