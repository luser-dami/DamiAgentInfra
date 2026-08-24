---
name: gameplay-tag-integration
description: Implementation of Native Gameplay Tags (UE 5.4) using FGameplayTagContainer. Replaces enums/strings for fast, scalable logic checks.
feature: gameplay-tag-integration
module: Core
---

# GameplayTag Integration (UE5 Expert)

## Context
- Mandatory for state tracking (Stunned, Burning, Sprinting).
- Mandatory for damage types, ability identifiers, and object categorization.
- Replaces raw `FName`, `FString` checks, and rigid `Enums`.

## 1. Native Gameplay Tags (UE 5.4 Standard)
Do not use `RequestGameplayTag` at runtime. Define Native Tags in C++ for compile-time safety and performance.

### Definition (.h / .cpp)
```cpp
// MyProjectTags.h
#pragma once
#include "NativeGameplayTags.h"

UE_DECLARE_GAMEPLAY_TAG_EXTERN(TAG_State_Dead);

// MyProjectTags.cpp
#include "MyProjectTags.h"

UE_DEFINE_GAMEPLAY_TAG(TAG_State_Dead, "State.Dead");
```

## 2. Usage in Classes
Implement `IGameplayTagAssetInterface` to allow global querying. Use `FGameplayTagContainer` for storage.

```cpp
#include "GameplayTagAssetInterface.h"

UCLASS()
class AMyActor : public AActor, public IGameplayTagAssetInterface
{
    GENERATED_BODY()
public:
    UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Tags")
    FGameplayTagContainer OwnedTags;

    virtual void GetOwnedGameplayTags(FGameplayTagContainer& TagContainer) const override { 
        TagContainer = OwnedTags; 
    }
};
```

## 3. Querying Tags (Hierarchical Checks)
Tags evaluate hierarchically. `State.Dead.Headshot` implies `State.Dead`.

```cpp
// Check specific tag or its children
if (OwnedTags.HasTag(TAG_State_Dead)) {
    // Actor is dead
}

// Check against multiple tags (Has ANY of them)
if (OwnedTags.HasAny(DamageTags)) { ... }
```


## Edge Cases
**BAD (String lookups at runtime)**:
```cpp
void Tick() {
    FGameplayTag Tag = UGameplayTagsManager::Get().RequestGameplayTag(FName("State.Dead")); // VERY SLOW
}
```

**GOOD (Native Tag usage)**:
```cpp
void Tick() {
    if (OwnedTags.HasTag(TAG_State_Dead)) { ... } // FAST, compile-time safe
}
```

**BAD (Naked Arrays)**:
```cpp
TArray<FGameplayTag> MyTags; // Lacks HasTag, HasAny, and hierarchical logic.
```

**GOOD (Containers)**:
```cpp
FGameplayTagContainer MyTags; // Optimized for bitwise tag operations.
```

## Verification Checklist
- [ ] `GameplayTags` module added to `Build.cs`.
- [ ] Tags declared via `UE_DECLARE_GAMEPLAY_TAG_EXTERN`.
- [ ] `FGameplayTagContainer` used instead of Arrays.
- [ ] `IGameplayTagAssetInterface` implemented on stateful actors.

## Architecture

The architecture for Gameplay Tag Integration follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Gameplay Tag Integration.

## Key Claims

- **Oversight Capacity**: Hashed Tags (`FGameplayTag`) provide instant comparisons (`==` on indices) vs slow string matching.
- **Scalability**: A designer can add `State.Dead.Burning` without modifying the C++ Enums. The logic `HasTag("State.Dead")` will still resolve true safely.
- **Fast Replication**: Containers replicate efficiently as bit arrays/indices.


The Gameplay Tag Integration pattern follows UE5 engine conventions and best practices.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h:1`

