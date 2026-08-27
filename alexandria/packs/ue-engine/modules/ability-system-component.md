---
name: ability-system-component
description: UAbilitySystemComponent internals — the activation pipeline, ability-spec replication, gameplay-event routing, and how prediction fits together.
module: ability-system-component
---

# AbilitySystemComponent Internals (UE5 Expert)

## Context
- Read before debugging ability activation failures, grant/lose-ability bugs, or "works on server, not on client" symptoms in GAS.
- Companion: [gas-ugameplayability](../domains/gas/gas-ugameplayability.md) (ability lifecycle), [gas-ability-replication-instancing](../domains/gas/gas-ability-replication-instancing.md) (policy matrix). This doc is the container those abilities live in.

## 1. What the ASC Owns

`UAbilitySystemComponent` is an actor component that owns four replicated containers plus the RPC surface for all of them:

- **ActivatableAbilities** — a fast array of `FGameplayAbilitySpec` (grant state, level, input binding, per-spec ability instances).
- **ActiveGameplayEffects** — fast array of applied GE specs.
- **Owned gameplay tags** — the aggregated tag container (loose + GE-granted).
- **Attributes** — registered `UAttributeSet` subobjects.

## 2. Activation Pipeline

```
GiveAbility (server) → FGameplayAbilitySpec added to the fast array → replicates to owner
        │
Input / code → TryActivateAbility(Handle)
        │
        ▼
CanActivateAbility: tags (required/blocked), cost, cooldown, instancing rules
        │
        ▼
CommitAbility (cost GE + cooldown GE applied)
        │
        ▼
ActivateAbility on the instance dictated by InstancingPolicy
        │
        ▼
EndAbility → instance freed or returned to the spec's instance list
```

`HandleGameplayEvent` is the second activation entry: it walks specs whose `AbilityTriggers` match the event tag and activates them. WaitGameplayEvent tasks listen on the same path without activating.

## 3. Ability Replication

- The **spec** replicates through the fast array — grant, removal, level, input pressed state.
- **Ability instances** replicate as subobjects: `ReplicateSubobjects` walks the spec's `ReplicatedInstances` list. An ability lands in that list only when its `ReplicationPolicy` is ReplicateYes; otherwise it sits in `NonReplicatedInstances` and never becomes a replicated subobject — custom RPCs on such abilities have no channel.
- Activation itself replicates: LocalPredicted abilities run on the owning client immediately and on the server when the RPC arrives; ServerInitiated abilities activate on the server and the activation replicates to the owning client.

## 4. Prediction (short version)

The ASC allocates a prediction key for client-side predicted activations and cost/cooldown application. Server accepts or rejects the predicted action; on rejection the client undoes the ability and its effects. Prediction covers activation, GE application, and triggers — not arbitrary state: anything your ability writes outside GAS (movement members, component flags) must reach both sides by your own symmetric path.

## 5. Tag Containers (why your tag query lies)

`GetOwnedGameplayTags` returns explicit tags; the aggregated count map is a different view. Loose tags (`AddLooseGameplayTag`) are refcounted; GE-granted tags live in the same explicit container — there is no loose-only query. Code that gates on tag presence must know which writer owns the tag, or a window stays open forever.

## Verification Checklist

- Ability won't activate: `AbilitySystem.Debug.NextTarget` / log categories LogGameplayAbilities — check blocked/required tags first, then cost/cooldown, then input binding on the spec.
- RPC on an ability never fires: check `ReplicationPolicy` — the validator error is literal, it does not "actually work" otherwise.
- Desync after activation: confirm both sides activated (LocalPredicted/ServerInitiated), not just one.

## Architecture

The ASC is the only GAS object that talks to the network. Abilities, tasks, effects, and cues are all dumb objects behind it; their replication goes through the ASC's fast arrays, its subobject replication, or its RPCs. Placing network logic anywhere else in GAS is fighting the framework.

## Data Flow

```
Grant (server) → spec fast array → replicated to owning client
        │
Activate (input/event/code)
        ├─ owning client: immediate, prediction key issued
        ├─ server: on RPC / locally initiated
        └─ simulated proxies: see only the replicated spec state + cues
        │
GameplayEvent → HandleGameplayEvent → matching AbilityTriggers activate
        │
ReplicateSubobjects → ability instances (ReplicateInstances list only)
```

## Key Claims

- [extracted] `UAbilitySystemComponent::GiveAbility` is defined at `Plugins/Runtime/GameplayAbilities/Source/GameplayAbilities/Private/AbilitySystemComponent_Abilities.cpp` and adds the spec to the replicated fast array.
- [extracted] `UAbilitySystemComponent::TryActivateAbility` is defined at `Plugins/Runtime/GameplayAbilities/Source/GameplayAbilities/Private/AbilitySystemComponent_Abilities.cpp` and is the code activation entry.
- [extracted] `UAbilitySystemComponent::HandleGameplayEvent` is defined at `Plugins/Runtime/GameplayAbilities/Source/GameplayAbilities/Private/AbilitySystemComponent_Abilities.cpp` and activates abilities whose AbilityTriggers match the event tag.
- [extracted] `UAbilitySystemComponent::ReplicateSubobjects` is defined at `Plugins/Runtime/GameplayAbilities/Source/GameplayAbilities/Private/AbilitySystemComponent.cpp` and replicates ability instances from the spec's replicated list.
- [inferred] Every GAS network path converges on the ASC, so ability-scoped RPCs are really ASC RPCs with an ability subobject as the payload.

## Edge Cases

- Granting an ability on the client is not replicated — grants are server-authoritative; the client-side spec only exists via replication.
- Removing an ability set cancels its active abilities; EndAbility runs on the cancel path — restore logic belongs there, not only in the happy path.
- `ReplicateSubobjects` skips abilities in the spec's non-replicated list, which is the silent half of the ReplicationPolicy contract.

## Boundaries

- The ASC does not own gameplay cues (UGameplayCueManager does) and does not compute damage (execution calculations and attribute sets do).
- Montage replication for abilities rides a separate struct path, not the ability instance channel.
- GE application internals live outside the ASC — see [gas-ugameplayeffect](../domains/gas/gas-ugameplayeffect.md).

## Evidence

- `UAbilitySystemComponent` defined at `Plugins/Runtime/GameplayAbilities/Source/GameplayAbilities/Public/AbilitySystemComponent.h`
- `FGameplayAbilitySpec` defined at `Plugins/Runtime/GameplayAbilities/Source/GameplayAbilities/Public/GameplayAbilitySpec.h`
- `UAbilitySystemComponent::GiveAbility` defined at `Plugins/Runtime/GameplayAbilities/Source/GameplayAbilities/Private/AbilitySystemComponent_Abilities.cpp`
- `UAbilitySystemComponent::TryActivateAbility` defined at `Plugins/Runtime/GameplayAbilities/Source/GameplayAbilities/Private/AbilitySystemComponent_Abilities.cpp`
- `UAbilitySystemComponent::HandleGameplayEvent` defined at `Plugins/Runtime/GameplayAbilities/Source/GameplayAbilities/Private/AbilitySystemComponent_Abilities.cpp`
- `UAbilitySystemComponent::ReplicateSubobjects` defined at `Plugins/Runtime/GameplayAbilities/Source/GameplayAbilities/Private/AbilitySystemComponent.cpp`
