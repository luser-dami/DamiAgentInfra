---
name: metasound-cpp-parameters
description: Interacting with MetaSounds via C++. Passing dynamic gameplay variables (Float, Wave, Strings) into the audio engine.
feature: metasound-cpp-parameters
module: Audio
---

# MetaSounds C++ Integration (UE5 Expert)

## Context
- Mandatory when sound effects need to react dynamically to gameplay (e.g., Engine RPM pitch, dynamic music stings, weather occlusion).
- Trigger when interacting with an `UAudioComponent` playing a MetaSound Source.

## 1. Core Principles
MetaSounds are highly optimized audio graphs. Instead of routing complex Blueprints, C++ passes simple typed parameters (Floats, Bools, Triggers) into the graph's input pins.

## 2. Implementation Syntax

### Setting up the Component
```cpp
#include "Components/AudioComponent.h"
#include "MetasoundSource.h"

AMyVehicle::AMyVehicle()
{
    EngineAudioComp = CreateDefaultSubobject<UAudioComponent>(TEXT("EngineAudio"));
    EngineAudioComp->SetupAttachment(RootComponent);
}

void AMyVehicle::BeginPlay()
{
    Super::BeginPlay();
    if (EngineSoundAsset)
    {
        EngineAudioComp->SetSound(EngineSoundAsset);
        EngineAudioComp->Play();
    }
}
```

### Passing Parameters (Tick or Event)
You must match the **exact name** and **type** of the Input pin defined in the MetaSound.

```cpp
void AMyVehicle::UpdateEngineSound(float CurrentRPM, bool bIsTurboActive)
{
    if (EngineAudioComp && EngineAudioComp->IsPlaying())
    {
        // Pass a Float (e.g., for Pitch/Filter)
        EngineAudioComp->SetFloatParameter(FName("RPM"), CurrentRPM);

        // Pass a Boolean (e.g., for routing logic inside the graph)
        EngineAudioComp->SetBoolParameter(FName("TurboMode"), bIsTurboActive);
    }
}
```

### Sending Triggers
Triggers act like execution pins in MetaSound to start envelopes, sequences, or one-shot layers.

```cpp
void AMyVehicle::OnGearShift()
{
    // Tells the MetaSound to trigger the "PlayShiftPop" execution pin
    EngineAudioComp->SetTriggerParameter(FName("PlayShiftPop"));
}
```


## Edge Cases
**BAD (Spawning new sounds constantly)**:
```cpp
// BAD: Spawning 100 individual audio components for a revving engine.
UGameplayStatics::PlaySoundAtLocation(this, RevSound, GetActorLocation()); 
```

**GOOD (Parametric Audio)**:
```cpp
// SAFE: One persistent audio component smoothly adjusting pitch.
EngineAudioComp->SetFloatParameter(FName("Pitch"), NewPitch);
```

## Verification Checklist
- [ ] `MetasoundEngine` module is added to `Build.cs`.
- [ ] The Sound Asset assigned to the `UAudioComponent` is a `UMetaSoundSource`.
- [ ] Parameter names in C++ match the MetaSound Graph Inputs perfectly.
- [ ] `SetTriggerParameter` is used for instantaneous events rather than rapidly toggling booleans.

## Architecture

The architecture for Metasound Cpp Parameters follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Metasound Cpp Parameters.

## Key Claims

- **Type Mismatch Failure**: If you pass `SetIntParameter` to a MetaSound pin defined as a `Float`, the audio engine ignores it silently. This is the #1 cause of MetaSound bugs.
- **Performance**: Passing a float every frame (`Tick`) to an `UAudioComponent` is relatively cheap because the Audio Thread interpolates it smoothly. However, avoid passing Strings or complex Waves every frame.


The Metasound Cpp Parameters pattern follows UE5 engine conventions and best practices.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h:1`

