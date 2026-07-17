---
architecture: Lyra
tags: [lyra, unreal, gas, overview, entry-point]
source: manual
---

# Lyra Project Architecture

Lyra is Epic's Unreal Engine sample game: a third-person shooter built almost
entirely on the **Gameplay Ability System (GAS)**, with gameplay features
delivered through modular **GameFeature plugins** on top of a shared
`LyraGame` core module. This document is the entry point for an agent entering
the codebase — read it first, then descend into domain/module documents.

## Context

- **Core module path:** `Source/LyraGame/`
- **Plugin content:** `Plugins/GameFeatures/` (ShooterCore, ShooterMaps, …)
- **Engine:** Unreal Engine 5, C++ with UCLASS/USTRUCT reflection everywhere
- **Key engine systems:** Gameplay Ability System, CommonUI, Enhanced Input

## Architecture

The core module is organized by concern, one folder per subsystem:

```
Source/LyraGame/
  AbilitySystem/   ASC, attribute sets (health, combat), gameplay abilities base
  Character/       ALyraCharacter, pawn extension, hero component, health component
  Weapons/         weapon instances, ranged weapon ability, weapon spawner
  Equipment/       equipment manager, equipment instances, quick bar
  Inventory/       inventory manager, item instances/definitions
  GameModes/       game mode, experience manager (loading/activating features)
  Player/          player state, player controller, local player
  Input/           input component, input config (Enhanced Input mappings)
  Camera/          camera modes and assists
  UI/              HUD, indicators, CommonUI integration
```

Two conventions matter for navigation:

- **GAS-centric design.** Anything that changes gameplay state (damage,
  healing, abilities, weapon fire) flows through
  `ULyraAbilitySystemComponent`, GameplayAbilities, and GameplayEffects —
  never through direct property writes from gameplay code.
- **Experience-driven content.** `ULyraExperienceManagerComponent` loads a
  `ULyraExperienceDefinition` which activates GameFeature plugins and grants
  `PawnData`; concrete game modes are data, not new C++ classes.

### Class Responsibilities

| Class | Module | Role |
|-------|--------|------|
| `ALyraGameMode` | GameModes | Orchestrates experience load and match flow |
| `ULyraExperienceManagerComponent` | GameModes | Loads/activates experiences and GameFeatures |
| `ALyraCharacter` | Character | The pawn; hosts ability and pawn extension components |
| `ULyraAbilitySystemComponent` | AbilitySystem | Central hub for abilities, attributes, effects |
| `ALyraPlayerState` | Player | Holds the player's ASC, team, and loadout data |

## Key Claims

- [extracted] `ALyraGameMode` is defined at `Source/LyraGame/GameModes/LyraGameMode.h:37` and owns match-level orchestration.
- [extracted] `ULyraExperienceManagerComponent` is defined at `Source/LyraGame/GameModes/LyraExperienceManagerComponent.h:30` and drives experience loading.
- [extracted] `ALyraCharacter` is defined at `Source/LyraGame/Character/LyraCharacter.h:98` and is the single pawn class for all humanoid agents.
- [extracted] `ULyraAbilitySystemComponent` is defined at `Source/LyraGame/AbilitySystem/LyraAbilitySystemComponent.h:28` and mediates every gameplay-state change.
- [extracted] `ALyraPlayerState` is defined at `Source/LyraGame/Player/LyraPlayerState.h:51` and carries per-player GAS state.
- [inferred] Gameplay variety (weapons, modes, maps) is intended to come from data and GameFeature plugins, so the core C++ module stays small and stable.
- [inferred] The folder-per-subsystem layout is the authoritative module boundary used by all knowledge documents in this project.

## Boundaries

- This document does **not** cover GameFeature plugin internals (ShooterCore and friends); those are content layered on the core.
- It does **not** document Unreal Engine's GAS itself — only Lyra's usage of it.
- It does **not** cover build/CI, cooking, or platform configuration.

## Evidence

- `ALyraGameMode` defined at `Source/LyraGame/GameModes/LyraGameMode.h:37`
- `ULyraExperienceManagerComponent` defined at `Source/LyraGame/GameModes/LyraExperienceManagerComponent.h:30`
- `ALyraCharacter` defined at `Source/LyraGame/Character/LyraCharacter.h:98`
- `ULyraAbilitySystemComponent` defined at `Source/LyraGame/AbilitySystem/LyraAbilitySystemComponent.h:28`
- `ALyraPlayerState` defined at `Source/LyraGame/Player/LyraPlayerState.h:51`
