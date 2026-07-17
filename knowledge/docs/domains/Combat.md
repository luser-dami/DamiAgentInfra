---
domain: Combat
tags: [combat, weapons, damage, abilities, health, gas]
source: manual
---

# Combat System

The Combat system is a cross-module view that ties together **Weapons**,
**AbilitySystem**, and **Character**. It describes the end-to-end flow from a
player pulling the trigger to a target taking damage and its health changing.

This document does not define concrete classes of its own; it is a
system-level orchestration map that references classes owned by other modules.

## Context

- **Scope:** system (spans multiple modules)
- **Systems involved:** Weapons, AbilitySystem, Character
- **Entry point:** player input → GameplayAbility activation
- **Dependencies:** GameplayAbilitySystem, GameplayEffects, GameplayTags
- **Consumers:** GameModes, UI (damage numbers, hit markers)

## Architecture

```
Player Input
   │
   ▼
ULyraGameplayAbility_RangedWeapon      ← ULyraRangedWeaponInstance (spread/heat/range)
   │   (weapon trace + targeting orchestration)
   ▼
GameplayEffect (Damage)
   │
   ▼
ULyraHealthSet                         ← Damage meta-attribute
   │   (clamps, converts Damage → Health)
   ▼
ULyraHealthComponent                   → broadcasts OnHealthChanged / handles death
```

## Data Flow

The full "weapon fires, hits a target, deals damage" flow:

```
Player fire input
      │
      ▼
ULyraGameplayAbility_RangedWeapon (ability activates)
      ├─ starts ranged weapon targeting
      ├─ performs weapon trace (line / sphere sweep) to find hits
      └─ on target data ready → applies a Damage GameplayEffect to the target ASC
                 │
                 ▼
      ULyraHealthSet (Damage meta-attribute aggregates incoming damage)
                 │
                 ▼
      PostGameplayEffectExecute (converts Damage into a Health reduction, clamped)
                 │
                 ▼
      ULyraHealthComponent (broadcasts OnHealthChanged; when health <= 0, triggers death)
                 │
                 ▼
      ULyraWeaponStateComponent (records damage instigator for client hit markers)
```

## Key Claims

- `ULyraGameplayAbility_RangedWeapon` is defined at `Source/LyraGame/Weapons/LyraGameplayAbility_RangedWeapon.h:47` and orchestrates the ranged firing flow.
- `ULyraHealthSet` is defined at `Source/LyraGame/AbilitySystem/Attributes/LyraHealthSet.h:32` and holds the Health and Damage attributes.
- `ULyraHealthComponent` is defined at `Source/LyraGame/Character/LyraHealthComponent.h:42` and mediates health changes and death.
- Damage is delivered through a GameplayEffect targeting the victim's AbilitySystemComponent, not by directly writing health.
- The Damage meta-attribute is converted into an actual Health reduction inside the attribute set's post-execute step.
- `ULyraWeaponStateComponent` tracks which pawn instigated damage so the server can confirm client-side hit markers.

## Boundaries

- This system does **not** cover melee combat; the current weapon flow is hitscan/ranged only.
- It does **not** define concrete damage numbers; those live in GameplayEffect data assets.
- It does **not** describe how AI decides to fire — only the mechanical damage pipeline.
- Health regeneration and revive flows are owned by the Character module, not described here.

## Evidence

- `ULyraGameplayAbility_RangedWeapon` defined at `Source/LyraGame/Weapons/LyraGameplayAbility_RangedWeapon.h:47`
- `ULyraRangedWeaponInstance` defined at `Source/LyraGame/Weapons/LyraRangedWeaponInstance.h:20`
- `ULyraWeaponStateComponent` defined at `Source/LyraGame/Weapons/LyraWeaponStateComponent.h:41`
- `ULyraHealthSet` defined at `Source/LyraGame/AbilitySystem/Attributes/LyraHealthSet.h:32`
- `ULyraHealthComponent` defined at `Source/LyraGame/Character/LyraHealthComponent.h:42`
