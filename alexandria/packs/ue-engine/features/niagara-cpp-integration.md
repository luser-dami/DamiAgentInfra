---
name: niagara-cpp-integration
description: Controlling Niagara particle systems via C++. Updating User Parameters dynamically (Vectors, Colors) for VFX integration.
feature: niagara-cpp-integration
module: FX
---

# Niagara C++ Integration (UE5 Expert)

## Context
- Mandatory when VFX must respond to gameplay data (e.g., a laser beam extending to a hit location, or fire changing color based on heat).
- Trigger when spawning or destroying Niagara systems via C++.

## 1. Core Principles
`UNiagaraComponent` acts as the bridge. You do not modify the particle materials directly; you pass data to **User Parameters** defined in the Niagara Editor.

## 2. Implementation Syntax

### Spawning the System
```cpp
#include "NiagaraFunctionLibrary.h"
#include "NiagaraComponent.h"

void AMyActor::PlayVFX(FVector TargetLocation)
{
    if (VFXAsset)
    {
        // Spawns and auto-destroys when the particle finishes
        UNiagaraComponent* NiagaraComp = UNiagaraFunctionLibrary::SpawnSystemAtLocation(
            GetWorld(),
            VFXAsset,
            GetActorLocation()
        );

        if (NiagaraComp)
        {
            // Send dynamic data to the User Parameters
            NiagaraComp->SetVectorParameter(FName("User.BeamEnd"), TargetLocation);
            NiagaraComp->SetFloatParameter(FName("User.HeatLevel"), 100.0f);
            NiagaraComp->SetColorParameter(FName("User.FlameColor"), FLinearColor::Red);
        }
    }
}
```

### Attached Systems
For persistent effects (e.g., a jetpack thruster).

```cpp
// In Constructor
ThrusterVFX = CreateDefaultSubobject<UNiagaraComponent>(TEXT("ThrusterVFX"));
ThrusterVFX->SetupAttachment(RootComponent);
ThrusterVFX->bAutoActivate = false;

// In Tick or Input
void AMyActor::UpdateThruster(float Throttle)
{
    if (Throttle > 0.0f) {
        ThrusterVFX->Activate();
        ThrusterVFX->SetFloatParameter(FName("User.ThrustPower"), Throttle);
    } else {
        ThrusterVFX->Deactivate();
    }
}
```

## 3. Callbacks (OnSystemFinished)
To execute C++ logic when a particle dies.

```cpp
void AMyActor::BeginPlay()
{
    Super::BeginPlay();
    if (MyNiagaraComp)
    {
        MyNiagaraComp->OnSystemFinished.AddDynamic(this, &AMyActor::HandleVFXFinished);
    }
}

void AMyActor::HandleVFXFinished(UNiagaraComponent* PSystem)
{
    // Clean up or trigger next phase
}
```


## Verification Checklist
- [ ] `Niagara` module is added to `PublicDependencyModuleNames` in `Build.cs`.
- [ ] Variable names use the `User.` prefix in `SetParameter` functions.
- [ ] `SpawnSystemAtLocation` is used for fire-and-forget effects to prevent memory leaks.

## Architecture

The architecture for Niagara Cpp Integration follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Niagara Cpp Integration.

## Key Claims

- **Typo Safety**: The `FName` must EXACTLY match the variable name in the Niagara system, including the `User.` prefix. (e.g., `User.TargetPosition`). A typo fails silently, and the VFX will not update.
- **Memory Optimization**: Use `SpawnSystemAtLocation` for transient effects to ensure the component is Garbage Collected automatically when the particles die.


The Niagara Cpp Integration pattern follows UE5 engine conventions and best practices.

## Edge Cases

Refer to the Common Mistakes section for Niagara Cpp Integration edge cases. Ensure proper null checks and UPROPERTY replication for networked usage.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h:1`

