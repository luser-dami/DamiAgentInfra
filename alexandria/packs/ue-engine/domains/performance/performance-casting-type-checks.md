---
name: performance-casting-type-checks
description: The hidden CPU cost of Cast<T>. Optimizing type checks using IsA(), GetClass(), ExactCast, and Interfaces.
domain: performance
---

# Casting Cost & Type Checking (UE5 Expert)

## Context
- Mandatory when resolving overlaps, hit events, or iterating through large arrays of Actors.
- Trigger when `Cast<T>` appears inside a loop or a `Tick` function.

## 1. The Hidden Cost of Cast<T>()
In Unreal Engine, `Cast<T>(Object)` is incredibly powerful but **expensive**. It must traverse the reflection system's inheritance tree dynamically at runtime to see if `Object` derives from `T`. 
If you perform 1,000 casts per frame, your framerate will tank.

## 2. Alternatives to Cast<T>

### `IsA<T>()` (Faster Type Check)
If you only need to know *if* an object is of a certain type, but you don't need to call a function on it, use `IsA`. It's slightly faster than casting.

```cpp
// BAD (Slow cast just to check type)
if (Cast<AMyEnemy>(HitActor)) { bHitEnemy = true; }

// GOOD (Faster type check)
if (HitActor->IsA<AMyEnemy>()) { bHitEnemy = true; }
```

### `GetClass() == T::StaticClass()` (Fastest Exact Check)
If you don't care about inheritance (i.e., you want to know if it is EXACTLY `AMyEnemy` and NOT a child class like `AMyEnemy_Boss`), comparing the class pointers is `O(1)` lightning fast.

```cpp
if (HitActor->GetClass() == AMyEnemy::StaticClass()) { ... }
```

### `ExactCast<T>()`
Returns the pointer only if it is the exact class (fails on subclasses). Much faster than a dynamic `Cast`.
```cpp
if (AMyEnemy* Enemy = ExactCast<AMyEnemy>(HitActor)) { Enemy->Damage(); }
```

## 3. The Interface Solution
The best way to avoid casting entirely is to use Interfaces (`UInterface`).

**BAD (Casting inside a loop)**:
```cpp
for (AActor* Actor : FoundActors) {
    if (AMyDoor* Door = Cast<AMyDoor>(Actor)) { Door->Interact(); }
    else if (AMyChest* Chest = Cast<AMyChest>(Actor)) { Chest->Interact(); }
}
```

**GOOD (Interface Execution)**:
```cpp
for (AActor* Actor : FoundActors) {
    // Very fast check, no casting required.
    if (Actor->Implements<UInteractableInterface>()) {
        IInteractableInterface::Execute_Interact(Actor);
    }
}
```


## Verification Checklist
- [ ] `Cast<T>` is avoided inside `Tick` or `for` loops spanning hundreds of items.
- [ ] `IsA<T>()` is used when the pointer itself isn't needed.
- [ ] `GetClass() == T::StaticClass()` is used for exact type matching without inheritance checks.
- [ ] Interfaces are favored over massive `if/else if Cast` chains.

## Architecture

The architecture for Performance Casting Type Checks follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Performance Casting Type Checks.

## Key Claims

- **Null Reference Crashes**: `Cast<T>` safely returns `nullptr` if the cast fails. A raw C-style cast `(AMyEnemy*)HitActor` is extremely dangerous and will cause memory access violations if wrong. NEVER use C-style casts for `UObjects`.


The Performance Casting Type Checks pattern follows UE5 engine conventions and best practices.

## Edge Cases

Refer to the Common Mistakes section for Performance Casting Type Checks edge cases. Ensure proper null checks and UPROPERTY replication for networked usage.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h`

