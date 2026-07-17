---
module: LyraGame/Weapons
tags: [weapons, ranged, spawner, equipment, gas]
source: manual
---

# Weapons Module

The Weapons module owns everything about a weapon as a runtime object: the
weapon instance and its tunables, the ranged-firing gameplay ability, heat and
spread state, and the world-side weapon spawner pickups. It sits on top of the
Equipment module, which provides the equip/unequip machinery.

## Context

- **Module path:** `Source/LyraGame/Weapons/`
- **Dependencies:** Equipment (base instance class), AbilitySystem (GAS), Inventory (item definitions)
- **Consumers:** Combat domain, Character (via equipment), UI (ammo/spread readouts)

## Architecture

```
ULyraEquipmentInstance (Equipment module)
   │
   ▼
ULyraWeaponInstance            ← base: anim layers, cosmetic, device properties
   │
   ▼
ULyraRangedWeaponInstance      ← adds heat/spread curves, range, trace params
   │
   ▼  grants, via ability set
ULyraGameplayAbility_RangedWeapon   ← performs traces, applies damage effects

World side:
ALyraWeaponSpawner             ← pickup actor using ULyraWeaponPickupDefinition
ULyraWeaponStateComponent      ← controller component: firing heat + instigator
```

### Class Responsibilities

| Class | Parent | Role |
|-------|--------|------|
| `ULyraWeaponInstance` | `ULyraEquipmentInstance` | Base weapon: cosmetics, anim layers, firing interface |
| `ULyraRangedWeaponInstance` | `ULyraWeaponInstance` | Hitscan/ranged params: spread, heat, range, trace sweep |
| `ULyraGameplayAbility_RangedWeapon` | `ULyraGameplayAbility` | Orchestrates targeting traces and damage GameplayEffects |
| `ULyraWeaponStateComponent` | `UControllerComponent` | Per-controller firing state and damage instigation |
| `ALyraWeaponSpawner` | `AActor` | World pickup that grants a weapon inventory item |

## Data Flow

From pickup to a weapon that can fire:

```
ALyraWeaponSpawner (overlap with pawn)
      │
      ▼
ULyraWeaponPickupDefinition → grants ULyraInventoryItemInstance (Inventory)
      │
      ▼
ULyraEquipmentManagerComponent equips it → spawns ULyraWeaponInstance
      │
      ▼
ULyraRangedWeaponInstance::OnEquipped seeds heat/spread state
      │
      ▼
Ability set grants ULyraGameplayAbility_RangedWeapon to the pawn's ASC
```

## Key Claims

- [extracted] `ULyraWeaponInstance` is defined at `Source/LyraGame/Weapons/LyraWeaponInstance.h:25` and extends the Equipment module's instance base class.
- [extracted] `ULyraRangedWeaponInstance` is defined at `Source/LyraGame/Weapons/LyraRangedWeaponInstance.h:20` and carries all ranged tunables (spread, heat, range).
- [extracted] `ALyraWeaponSpawner` is defined at `Source/LyraGame/Weapons/LyraWeaponSpawner.h:25` and grants weapons through pickup definitions.
- [extracted] `ULyraWeaponStateComponent` is defined at `Source/LyraGame/Weapons/LyraWeaponStateComponent.h:41` and lives on the controller, not the pawn.
- [inferred] Firing capability reaches the pawn only through granted abilities, so stripping the ability set fully disarms the pawn without destroying the weapon instance.
- [inferred] Weapon state is split deliberately: instance-owned tunables replicate with the equipment, while transient firing heat is controller-side via the state component.

## Boundaries

- The Weapons module does **not** implement equip/inventory mechanics — those live in the Equipment and Inventory modules.
- The Weapons module does **not** define damage numbers or hit resolution; the ability applies data-driven GameplayEffects (see the Combat domain).
- The Weapons module does **not** cover melee weapons; only ranged/hitscan weapon behavior exists here.
- Ammunition and reload UX are not modeled in this module's C++ classes.

## Evidence

- `ULyraWeaponInstance` defined at `Source/LyraGame/Weapons/LyraWeaponInstance.h:25`
- `ULyraRangedWeaponInstance` defined at `Source/LyraGame/Weapons/LyraRangedWeaponInstance.h:20`
- `ULyraGameplayAbility_RangedWeapon` defined at `Source/LyraGame/Weapons/LyraGameplayAbility_RangedWeapon.h:47`
- `ULyraWeaponStateComponent` defined at `Source/LyraGame/Weapons/LyraWeaponStateComponent.h:41`
- `ALyraWeaponSpawner` defined at `Source/LyraGame/Weapons/LyraWeaponSpawner.h:25`
