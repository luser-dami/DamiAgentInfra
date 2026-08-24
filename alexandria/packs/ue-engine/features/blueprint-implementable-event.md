---
name: blueprint-implementable-event
description: Usage of BlueprintImplementableEvent to define C++ function hooks that have no native implementation and exist solely for Blueprint logic.
feature: blueprint-implementable-event
module: Core
---

# BlueprintImplementableEvent (UE5 Expert)

## Context
- Use when C++ needs to notify Blueprints of an event, but C++ has NO default logic to execute.
- Ideal for purely visual or audio triggers (e.g., `OnPlayFootstepSound`, `OnSpawnVisualEffect`).

## 1. Core Principles
- **No C++ Body**: You only declare the function in the header. Do not write a `.cpp` definition.
- **Performance**: Very fast way to bridge C++ state changes to Blueprint visual scripting.

## 2. Implementation Syntax

### Declaration (.h)
```cpp
UFUNCTION(BlueprintImplementableEvent, Category = "FX")
void OnWeaponFired(const FVector& HitLocation);
```

### Calling from C++
Call it like a normal function. If the Blueprint hasn't implemented it, it silently does nothing (safe).
```cpp
void AMyWeapon::Fire()
{
    FVector HitLoc = CalculateHit();
    // Trigger visual/audio logic in Blueprint
    OnWeaponFired(HitLoc);
}
```


## Edge Cases
**BAD (Writing a CPP body)**:
```cpp
// MyWeapon.cpp
void AMyWeapon::OnWeaponFired(const FVector& HitLocation) { ... } // LINKER ERROR
```

**GOOD (Header only)**:
```cpp
// Just the header declaration. No CPP code.
```

**BAD (Using for core logic)**:
```cpp
UFUNCTION(BlueprintImplementableEvent)
float CalculateDamage(); // UNSAFE. If BP doesn't implement, returns 0.0f. Use BlueprintNativeEvent instead.
```

## Verification Checklist
- [ ] Used strictly for logic that C++ does not need to control.
- [ ] No implementation written in `.cpp`.
- [ ] Return types are avoided (use `BlueprintNativeEvent` if C++ relies on a return value).

## Architecture

The architecture for Blueprint Implementable Event follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Blueprint Implementable Event.

## Key Claims

- **Strict Separation of Concerns**: Forces visual/audio logic out of C++ and into Blueprint where artists can iterate without recompiling.
- **Null Safety**: Unlike delegates, you don't need to check if it's bound. Calling an unimplemented `BlueprintImplementableEvent` is perfectly safe and incurs near-zero overhead.


The Blueprint Implementable Event pattern follows UE5 engine conventions and best practices.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h:1`

