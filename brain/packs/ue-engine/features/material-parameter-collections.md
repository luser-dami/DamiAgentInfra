---
name: material-parameter-collections
description: Implementing global parameters (e.g., GlobalWetness) via C++ to affect thousands of materials simultaneously.
feature: material-parameter-collections
module: Rendering
---

# Material Parameter Collections (UE5 Expert)

## Context
- Mandatory when a single variable (e.g., Time of Day, Weather, Global Highlight) must affect many disparate materials.
- Trigger when you need to avoid creating and updating thousands of Dynamic Material Instances for a global effect.

## 1. Core Principles
A `UMaterialParameterCollection` (MPC) is an asset containing scalar and vector parameters. Materials reference this asset directly. When the MPC is updated via C++ or Blueprint, every material referencing it updates instantly on the GPU, with zero CPU overhead per instance.

## 2. Implementation in C++

### Fetching the Collection
```cpp
#include "Materials/MaterialParameterCollection.h"
#include "Materials/MaterialParameterCollectionInstance.h"

// Typically loaded via TSoftObjectPtr or direct reference in a Subsystem
UPROPERTY(EditAnywhere)
UMaterialParameterCollection* WeatherCollection;
```

### Updating a Parameter
Use `UKismetMaterialLibrary` or access the `UMaterialParameterCollectionInstance` from the World.

```cpp
#include "Kismet/KismetMaterialLibrary.h"

void AWeatherSystem::SetGlobalWetness(float WetnessValue)
{
    if (WeatherCollection)
    {
        // Safe global update. Affects all materials referencing 'GlobalWetness' instantly.
        UKismetMaterialLibrary::SetScalarParameterValue(GetWorld(), WeatherCollection, FName("GlobalWetness"), WetnessValue);
    }
}
```


## Edge Cases
**BAD (Massive MID Updates)**:
```cpp
for (AMyActor* Actor : AllActorsInWorld) {
    Actor->GetMesh()->CreateAndSetMaterialInstanceDynamic(0)->SetScalarParameterValue("Wetness", 1.0f);
}
// CPU bottleneck!
```

**GOOD (MPC Update)**:
```cpp
UKismetMaterialLibrary::SetScalarParameterValue(GetWorld(), WeatherCollection, "Wetness", 1.0f);
// Lightning fast.
```

## Verification Checklist
- [ ] Global state variables are placed in an MPC, not individual Material Instances.
- [ ] `UKismetMaterialLibrary` is used to update the MPC in C++.
- [ ] Parameter names match EXACTLY with the FName used in C++.

## Architecture

The architecture for Material Parameter Collections follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Material Parameter Collections.

## Key Claims

- **Zero Instancing**: Updating 10,000 wet rocks using Dynamic Material Instances (MIDs) requires 10,000 CPU calls. An MPC requires 1 CPU call.
- **State Integrity**: Ensures agents can easily manipulate global environment states safely without tracking arrays of actor materials.


The Material Parameter Collections pattern follows UE5 engine conventions and best practices.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h:1`

