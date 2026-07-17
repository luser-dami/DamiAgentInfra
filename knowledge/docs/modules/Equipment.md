---
module: LyraGame/Equipment
tags: [equipment, quickbar, inventory, abilities, replication]
source: manual
---

# Equipment Module

The Equipment module turns inventory items into active gameplay objects: it
spawns and destroys equipment instances, grants their ability sets to the
pawn's ability system component, and replicates the equipped list. The quick
bar and weapon spawner pickups are the player-facing edges of the module.

## Context

- **Module path:** `Source/LyraGame/Equipment/`
- **Dependencies:** AbilitySystem (ability sets), Inventory (item instances), Character (pawn)
- **Consumers:** Weapons module (weapon instances), game features, UI (quick bar slots)

## Architecture

```
ULyraEquipmentDefinition      (data asset: instance class + ability set + actors to spawn)
        │
        ▼
ULyraEquipmentManagerComponent   (on the pawn: spawns/destroys instances,
        │                          applies/removes ability sets, replicates list)
        ▼
ULyraEquipmentInstance           (runtime object: OnEquipped/OnUnequipped hooks)
        │
        ▼
ULyraGameplayAbility_FromEquipment   (ability that knows its source equipment/item)

Player-facing:
ULyraQuickBarComponent   (on the controller: slot switching → equip requests)
ULyraPickupDefinition / ULyraWeaponPickupDefinition   (pickup data assets)
```

### Class Responsibilities

| Class | Parent | Role |
|-------|--------|------|
| `ULyraEquipmentManagerComponent` | `UPawnComponent` | Equips/unequips items, owns the replicated equipment list |
| `ULyraEquipmentInstance` | `UObject` | Base runtime equipment object with equip hooks |
| `ULyraEquipmentDefinition` | `UObject` | Static data: instance class, ability set, spawned actors |
| `ULyraGameplayAbility_FromEquipment` | `ULyraGameplayAbility` | Ability granted by equipment, back-references its item |
| `ULyraQuickBarComponent` | `UControllerComponent` | Maps quick-bar slots to equip requests |

## Data Flow

Equip path, server-authoritative:

```
ULyraQuickBarComponent (slot input) or ULyraWeaponSpawner (pickup)
      │
      ▼
ULyraEquipmentManagerComponent::EquipItem  (authority only)
      ├─ spawns ULyraEquipmentInstance from ULyraEquipmentDefinition
      ├─ grants the definition's ability set to ULyraAbilitySystemComponent
      └─ adds an entry to the replicated FLyraEquipmentEntry list
                 │
                 ▼
      clients see the replicated list → spawn cosmetic actors locally
```

Unequip reverses it: ability set handles are removed, the instance is
destroyed, and the entry leaves the replicated list.

## Key Claims

- [extracted] `ULyraEquipmentManagerComponent` is defined at `Source/LyraGame/Equipment/LyraEquipmentManagerComponent.h:115` and owns the replicated equipment list.
- [extracted] `ULyraEquipmentInstance` is defined at `Source/LyraGame/Equipment/LyraEquipmentInstance.h:20` and is the base class for all runtime equipment.
- [extracted] `ULyraEquipmentDefinition` is defined at `Source/LyraGame/Equipment/LyraEquipmentDefinition.h:38` and binds an instance class to an ability set.
- [extracted] `ULyraQuickBarComponent` is defined at `Source/LyraGame/Equipment/LyraQuickBarComponent.h:17` and lives on the controller.
- [extracted] `ULyraGameplayAbility_FromEquipment` is defined at `Source/LyraGame/Equipment/LyraGameplayAbility_FromEquipment.h:18` and links an ability back to its source item.
- [inferred] Equipping is authority-only by design: clients never spawn instances themselves, they react to the replicated list, which keeps cosmetic state consistent.
- [inferred] The definition/instance split mirrors UE's data-asset pattern so designers add equipment without touching C++.

## Boundaries

- This module does **not** decide *what* an item does when used — that is the owning module's job (e.g. Weapons for firing).
- It does **not** implement inventory storage or stacking; items come from the Inventory module.
- It does **not** handle weapon pickup presentation (meshes, effects) beyond the pickup data assets.

## Evidence

- `ULyraEquipmentManagerComponent` defined at `Source/LyraGame/Equipment/LyraEquipmentManagerComponent.h:115`
- `ULyraEquipmentInstance` defined at `Source/LyraGame/Equipment/LyraEquipmentInstance.h:20`
- `ULyraEquipmentDefinition` defined at `Source/LyraGame/Equipment/LyraEquipmentDefinition.h:38`
- `ULyraGameplayAbility_FromEquipment` defined at `Source/LyraGame/Equipment/LyraGameplayAbility_FromEquipment.h:18`
- `ULyraQuickBarComponent` defined at `Source/LyraGame/Equipment/LyraQuickBarComponent.h:17`
