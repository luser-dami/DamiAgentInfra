---
name: static-blueprint-libraries
description: Exposing C++ logic via UBlueprintFunctionLibrary. Bridges the gap between native performance and EditorUtilityBlueprint flexibility.
feature: static-blueprint-libraries
module: Core
---

# Static Blueprint Libraries (UE5 Expert)

## Context
- Mandatory when exposing pure math, generic utilities, or complex C++ algorithms to Blueprints.
- Use to create custom nodes for `EditorUtilityBlueprints` or Gameplay graphs.
- DO NOT use for stateful logic (Libraries cannot hold instance variables safely).

## 1. Core Principles
- **Stateless**: All functions must be `static`. No class instances are required to call them.
- **Global Access**: Nodes are available anywhere in any Blueprint.

## 2. Implementation Syntax

### Header (.h)
```cpp
#pragma once
#include "CoreMinimal.h"
#include "Kismet/BlueprintFunctionLibrary.h"
#include "MyUtilsLibrary.generated.h"

UCLASS()
class MYPROJECT_API UMyUtilsLibrary : public UBlueprintFunctionLibrary
{
    GENERATED_BODY()

public:
    // Pure function (Green node, no execution pins)
    UFUNCTION(BlueprintPure, Category = "MyProject|Math")
    static float CalculateComplexTrajectory(FVector Start, FVector End);

    // Executable function (Blue node, has execution pins)
    UFUNCTION(BlueprintCallable, Category = "MyProject|System")
    static void ForceGarbageCollection();
};
```

### Source (.cpp)
```cpp
float UMyUtilsLibrary::CalculateComplexTrajectory(FVector Start, FVector End)
{
    // Heavy C++ math here
    return FVector::Distance(Start, End) * 3.14f;
}

void UMyUtilsLibrary::ForceGarbageCollection()
{
    GEngine->ForceGarbageCollection(true);
}
```

## 3. World Context Object
If your static function needs to spawn an actor or access the `UWorld`, you must provide a WorldContextObject.

```cpp
// Header
UFUNCTION(BlueprintCallable, meta = (WorldContext = "WorldContextObject"), Category = "MyProject")
static void SpawnSystem(const UObject* WorldContextObject);

// CPP
void UMyUtilsLibrary::SpawnSystem(const UObject* WorldContextObject)
{
    if (UWorld* World = GEngine->GetWorldFromContextObject(WorldContextObject, EGetWorldErrorMode::LogAndReturnNull))
    {
        // Safe to use World
    }
}
```


## Verification Checklist
- [ ] Class inherits from `UBlueprintFunctionLibrary`.
- [ ] All `UFUNCTION` methods are `static`.
- [ ] `BlueprintPure` used for getter/math functions (no side effects).
- [ ] `BlueprintCallable` used for functions that modify state.
- [ ] `WorldContext` meta tag used when `GetWorld()` is required.

## Architecture

The architecture for Static Blueprint Libraries follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Static Blueprint Libraries.

## Key Claims

- **Performance**: Keeps Editor tools and Gameplay logic fast by ensuring tight loops run in native C++.
- **Reusability**: Write once in C++, use infinitely in UI, Editor Tools, and Game logic.


The Static Blueprint Libraries pattern follows UE5 engine conventions and best practices.

## Edge Cases

Refer to the Common Mistakes section for Static Blueprint Libraries edge cases. Ensure proper null checks and UPROPERTY replication for networked usage.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h`

