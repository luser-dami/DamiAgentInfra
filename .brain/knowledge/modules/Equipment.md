---
module: LyraGame/Equipment
tags: [scaffold]
source: scaffold
---

# Equipment Module

TODO: one sentence on what this module provides. This draft was machine-scaffolded
from the code index (structure is real, semantics are placeholders); complete it
following AUTHORING.md, then delete this paragraph.

## Context

- **Module path:** `Source/LyraGame/Equipment/`
- **Dependencies:** `AbilitySystem/Abilities/LyraGameplayAbility.h`, `AbilitySystem/LyraAbilitySet.h`, `AbilitySystem/LyraAbilitySystemComponent.h`, `AbilitySystemGlobals.h`, `Components/ControllerComponent.h`, `Components/PawnComponent.h`, `Components/SkeletalMeshComponent.h`, `Engine/ActorChannel.h`, `Engine/DataAsset.h`, `Engine/World.h`, `Equipment/LyraEquipmentDefinition.h`, `Equipment/LyraEquipmentInstance.h`, `Equipment/LyraEquipmentManagerComponent.h`, `Equipment/LyraPickupDefinition.h`, `GameFramework/Character.h`, `GameFramework/GameplayMessageSubsystem.h`
- **Consumers:** `Plugins/GameFeatures/ShooterTests/Source/ShooterTestsRuntime/Private/ShooterTestsActorAnimationTests.cpp`, `Source/LyraGame/AbilitySystem/Abilities/LyraAbilityCost_ItemTagStack.cpp`, `Source/LyraGame/UI/Weapons/LyraWeaponUserInterface.cpp`, `Source/LyraGame/Weapons/LyraGameplayAbility_RangedWeapon.h`, `Source/LyraGame/Weapons/LyraWeaponInstance.h`, `Source/LyraGame/Weapons/LyraWeaponSpawner.cpp`, `Source/LyraGame/Weapons/LyraWeaponStateComponent.cpp`

## Architecture

### Class Responsibilities

| Class | Defined | Role |
|-------|---------|------|
| `FLyraEquipmentActorToSpawn` | Source/LyraGame/Equipment/LyraEquipmentDefinition.h:14 | TODO |
| `ULyraEquipmentDefinition` | Source/LyraGame/Equipment/LyraEquipmentDefinition.h:38 | TODO |
| `ULyraEquipmentInstance` | Source/LyraGame/Equipment/LyraEquipmentInstance.h:20 | TODO |
| `FOutBunch` | Source/LyraGame/Equipment/LyraEquipmentManagerComponent.cpp:181 | TODO |
| `FLyraAppliedEquipmentEntry` | Source/LyraGame/Equipment/LyraEquipmentManagerComponent.h:26 | TODO |
| `FLyraEquipmentList` | Source/LyraGame/Equipment/LyraEquipmentManagerComponent.h:53 | TODO |
| `TStructOpsTypeTraits` | Source/LyraGame/Equipment/LyraEquipmentManagerComponent.h:97 | TODO |
| `ULyraEquipmentManagerComponent` | Source/LyraGame/Equipment/LyraEquipmentManagerComponent.h:115 | TODO |
| `UActorChannel` | Source/LyraGame/Equipment/LyraEquipmentManagerComponent.h:129 | TODO |
| `ULyraGameplayAbility_FromEquipment` | Source/LyraGame/Equipment/LyraGameplayAbility_FromEquipment.h:18 | TODO |
| `FDataValidationContext` | Source/LyraGame/Equipment/LyraGameplayAbility_FromEquipment.h:33 | TODO |
| `ULyraPickupDefinition` | Source/LyraGame/Equipment/LyraPickupDefinition.h:19 | TODO |
| `ULyraWeaponPickupDefinition` | Source/LyraGame/Equipment/LyraPickupDefinition.h:56 | TODO |
| `ULyraQuickBarComponent` | Source/LyraGame/Equipment/LyraQuickBarComponent.h:17 | TODO |
| `FLyraQuickBarSlotsChangedMessage` | Source/LyraGame/Equipment/LyraQuickBarComponent.h:85 | TODO |
| `FLyraQuickBarActiveIndexChangedMessage` | Source/LyraGame/Equipment/LyraQuickBarComponent.h:98 | TODO |

## Data Flow

TODO: the end-to-end flow through this module. Use the real class and function
names listed above — bare CamelCase names extract as code anchors automatically.

Functions seen in this module: `Super`, `Super`, `IsSupportedForNetworking`, `GetWorld`, `GetInstigator`, `SetInstigator`, `GetPawn`, `GetTypedPawn`.

## Key Claims

- [inferred] TODO: state one self-contained design claim about the Equipment module, naming its subject.

## Boundaries

- The Equipment module does **not** TODO: name at least one explicit out-of-scope responsibility.

## Evidence

- `FLyraEquipmentActorToSpawn` defined at `Source/LyraGame/Equipment/LyraEquipmentDefinition.h:14`
- `ULyraEquipmentDefinition` defined at `Source/LyraGame/Equipment/LyraEquipmentDefinition.h:38`
- `ULyraEquipmentInstance` defined at `Source/LyraGame/Equipment/LyraEquipmentInstance.h:20`
- `FOutBunch` defined at `Source/LyraGame/Equipment/LyraEquipmentManagerComponent.cpp:181`
- `FLyraAppliedEquipmentEntry` defined at `Source/LyraGame/Equipment/LyraEquipmentManagerComponent.h:26`
- `FLyraEquipmentList` defined at `Source/LyraGame/Equipment/LyraEquipmentManagerComponent.h:53`
- `TStructOpsTypeTraits` defined at `Source/LyraGame/Equipment/LyraEquipmentManagerComponent.h:97`
- `ULyraEquipmentManagerComponent` defined at `Source/LyraGame/Equipment/LyraEquipmentManagerComponent.h:115`
