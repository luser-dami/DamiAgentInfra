---
name: physics-materials-cpp
description: Accessing Physical Materials from FHitResult to drive logic (Surface Types, Decals, Sounds).
feature: physics-materials-cpp
module: Physics
---

# Physics Materials in C++ (UE5 Expert)

## Context
- Mandatory when you need different reactions based on what surface was hit (e.g., Metal sparks, Flesh blood, Wood splinters).
- Trigger after a successful `LineTrace` or `Hit` event.

## 1. Core Principles
Physical Materials (`UPhysicalMaterial`) are assigned to Material Assets or Collision Components. They map to `EPhysicalSurface` (Surface Types defined in Project Settings).

## 2. Setup (Project Settings)
In `Project Settings -> Physics`, define your Physical Surfaces.
- `SurfaceType1` -> `Wood`
- `SurfaceType2` -> `Metal`
- `SurfaceType3` -> `Flesh`

## 3. Implementation Syntax

### Step 1: Force Trace to Return Material
You MUST tell the trace to gather the material, otherwise it returns null to save CPU.
```cpp
FCollisionQueryParams Params;
Params.bReturnPhysicalMaterial = true; // CRITICAL
```

### Step 2: Extract Surface Type
Use `UPhysicalMaterial::DetermineSurfaceType`.

```cpp
#include "PhysicalMaterials/PhysicalMaterial.h"

void AMyWeapon::ProcessHit(const FHitResult& Hit)
{
    // Safely get the weak pointer to the PhysMaterial
    UPhysicalMaterial* PhysMat = Hit.PhysMaterial.Get();
    
    // Determine the Surface Type (Returns SurfaceType_Default if PhysMat is null)
    EPhysicalSurface SurfaceType = UPhysicalMaterial::DetermineSurfaceType(PhysMat);

    // Switch based on surface
    switch (SurfaceType)
    {
        case SurfaceType1: // Wood
            PlayWoodVFX(Hit.ImpactPoint);
            break;
        case SurfaceType2: // Metal
            PlayMetalVFX(Hit.ImpactPoint);
            break;
        default:
            PlayDefaultVFX(Hit.ImpactPoint);
            break;
    }
}
```


## Verification Checklist
- [ ] `bReturnPhysicalMaterial = true` is set in the `FCollisionQueryParams`.
- [ ] `UPhysicalMaterial::DetermineSurfaceType` is used to safely evaluate the surface.
- [ ] Logic uses a `switch` or `if/else` block mapped to the Project Settings `SurfaceType` indices.

## Architecture

The architecture for Physics Materials Cpp follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Physics Materials Cpp.

## Key Claims

- **Null Reference Avoidance**: `Hit.PhysMaterial` is a `TWeakObjectPtr`. You must use `.Get()` and the engine's `DetermineSurfaceType` helper function, which safely handles nullptrs by returning `SurfaceType_Default`.
- **Decoupling**: Relying on Physical Materials is infinitely safer than checking `Hit.GetActor()->ActorHasTag("Metal")`, because a single actor (like a car) can have multiple materials (Glass, Metal, Rubber) on different polygons.


The Physics Materials Cpp pattern follows UE5 engine conventions and best practices.

## Edge Cases

Refer to the Common Mistakes section for Physics Materials Cpp edge cases. Ensure proper null checks and UPROPERTY replication for networked usage.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h`

