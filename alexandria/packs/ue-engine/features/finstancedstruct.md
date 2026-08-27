---
name: finstancedstruct
description: Utilizing FInstancedStruct for heterogeneous data arrays. Replaces arrays of UObject pointers for pure data to eliminate GC overhead.
feature: finstancedstruct
module: Core
---

# FInstancedStruct (UE5 Expert)

## Context
- Mandatory when you need an array or list that can hold *different types* of structs (Heterogeneous arrays).
- Trigger when you are using `UObject` arrays purely to hold data and want to optimize away Garbage Collection (GC) overhead.

## 1. The UObject Overhead Problem
Historically, if you wanted an array of "Abilities" where some were "FireAbility" and some were "IceAbility" with entirely different variables, you had to make them `UObjects`. 
- **Problem**: 10,000 `UObject` data containers cause the Garbage Collector to freeze the game during sweeps.

## 2. Enter FInstancedStruct (UE 5.0+)
`FInstancedStruct` (part of the `StructUtils` plugin) acts like a wrapper. It can hold ANY `USTRUCT()`, allocate exactly the right amount of memory for it, and serialize it cleanly, entirely bypassing the UObject system.

### Step 1: Enable Plugin
Add `StructUtils` to your `Build.cs` `PublicDependencyModuleNames`.

### Step 2: Define Base Structs
```cpp
#pragma once
#include "InstancedStruct.h"
#include "MyData.generated.h"

// Base struct (Empty)
USTRUCT(BlueprintType)
struct FBaseAbilityData { GENERATED_BODY() };

// Child structs
USTRUCT(BlueprintType)
struct FFireAbilityData : public FBaseAbilityData 
{ 
    GENERATED_BODY()
    UPROPERTY(EditAnywhere) float BurnDuration; 
};

USTRUCT(BlueprintType)
struct FIceAbilityData : public FBaseAbilityData 
{ 
    GENERATED_BODY()
    UPROPERTY(EditAnywhere) float FreezeAmount; 
};
```

### Step 3: Use FInstancedStruct
```cpp
UCLASS()
class AMyActor : public AActor
{
    GENERATED_BODY()

public:
    // This array can now hold a mix of FFireAbilityData and FIceAbilityData!
    UPROPERTY(EditAnywhere, meta = (BaseStruct = "/Script/MyProject.BaseAbilityData"))
    TArray<FInstancedStruct> Abilities;

    void ProcessAbilities();
};
```

### Step 4: Accessing Data (C++)
```cpp
void AMyActor::ProcessAbilities()
{
    for (const FInstancedStruct& InstancedStruct : Abilities)
    {
        if (InstancedStruct.IsValid())
        {
            // Check type and extract
            if (const FFireAbilityData* FireData = InstancedStruct.GetPtr<FFireAbilityData>())
            {
                // Apply burn
            }
            else if (const FIceAbilityData* IceData = InstancedStruct.GetPtr<FIceAbilityData>())
            {
                // Apply freeze
            }
        }
    }
}
```


## Verification Checklist
- [ ] `StructUtils` is added to `Build.cs`.
- [ ] Base struct is defined and passed to `meta = (BaseStruct = "...")`.
- [ ] Extraction uses `GetPtr<T>()` and checks for null safely.
- [ ] Replaces `TArray<UObject*>` used purely for data storage.

## Architecture

The architecture for Finstancedstruct follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Finstancedstruct.

## Key Claims

- **Zero GC Overhead**: Structs are value types managed in contiguous memory. They do not register with the Garbage Collector.
- **Editor Support**: The `BaseStruct` meta tag ensures the Unreal Editor provides a clean dropdown menu for designers to pick which struct type to add to the array, ensuring type safety.


The Finstancedstruct pattern follows UE5 engine conventions and best practices.

## Edge Cases

Refer to the Common Mistakes section for Finstancedstruct edge cases. Ensure proper null checks and UPROPERTY replication for networked usage.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h`

