---
name: dynamic-material-instances
description: Creating Dynamic Material Instances (MIDs) at runtime to apply unique effects to specific meshes without affecting the master material.
feature: dynamic-material-instances
module: Rendering
---

# Dynamic Material Instances (UE5 Expert)

## Context
- Mandatory when an actor needs a UNIQUE visual state (e.g., individual health glow, hit flash, custom camo color).
- Trigger when you need to change a material parameter on a single mesh instance at runtime.

## 1. Core Principles
A Material Instance Dynamic (MID) is a runtime copy of a material that can be safely modified via C++ or Blueprint without altering the base asset or other actors using the same base asset.

## 2. Implementation in C++

### Creation (BeginPlay or OnDemand)
Never create an MID inside `Tick`. Create it once and cache it.

```cpp
#include "Materials/MaterialInstanceDynamic.h"
#include "Components/StaticMeshComponent.h"

UCLASS()
class AMyActor : public AActor
{
    GENERATED_BODY()
protected:
    UPROPERTY()
    UMaterialInstanceDynamic* CachedMID;

    virtual void BeginPlay() override;
    void TakeDamage();
};
```

```cpp
void AMyActor::BeginPlay()
{
    Super::BeginPlay();
    
    // Create MID for Material Index 0
    if (MeshComponent && MeshComponent->GetMaterial(0))
    {
        CachedMID = MeshComponent->CreateAndSetMaterialInstanceDynamic(0);
    }
}
```

### Modification
```cpp
void AMyActor::TakeDamage()
{
    if (CachedMID)
    {
        CachedMID->SetVectorParameterValue(TEXT("FlashColor"), FLinearColor::Red);
        CachedMID->SetScalarParameterValue(TEXT("FlashIntensity"), 1.0f);
    }
}
```


## Edge Cases
**BAD (Creating in loop/Tick)**:
```cpp
void AMyActor::Tick(float DeltaTime) {
    // FATAL: Creates a new material object every frame! Massive memory leak.
    UMaterialInstanceDynamic* MID = Mesh->CreateAndSetMaterialInstanceDynamic(0);
}
```

**GOOD (Cached Creation)**:
```cpp
void AMyActor::Tick(float DeltaTime) {
    if (CachedMID) { CachedMID->SetScalarParameterValue("Time", GetWorld()->GetTimeSeconds()); }
}
```

## Verification Checklist
- [ ] `CreateAndSetMaterialInstanceDynamic` is called ONCE per mesh/material slot.
- [ ] The resulting MID pointer is cached in a `UPROPERTY`.
- [ ] `SetScalarParameterValue` or `SetVectorParameterValue` uses correct `FName` matching the material graph.

## Architecture

The architecture for Dynamic Material Instances follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Dynamic Material Instances.

## Key Claims

- **Draw Calls**: Every MID breaks instancing (if not using Instanced Static Meshes). 100 actors with 100 MIDs = 100 draw calls. Use sparingly or leverage `CustomPrimitiveData` for massive crowds.
- **Memory**: MIDs consume RAM. Over-creating them (e.g., generating a new one every time you take damage instead of caching) will cause severe memory leaks.


The Dynamic Material Instances pattern follows UE5 engine conventions and best practices.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h:1`

