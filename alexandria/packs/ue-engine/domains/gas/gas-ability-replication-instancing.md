---
name: gas-ability-replication-instancing
description: The GAS ability policy matrix — InstancingPolicy, NetExecutionPolicy, NetSecurityPolicy, and UE 5.6's ReplicationPolicy — what each combination actually does on the network.
domain: gas
---

# GAS: Ability Replication & Instancing Policies (UE5 Expert)

## Context
- Mandatory reading before adding any custom RPC to a GameplayAbility or picking a policy for a new ability.
- Trigger: the 5.6 validator error "RPC Functions require ReplicationPolicy to be ReplicateYes in order to actually work" — it is literal, not a suggestion.

## 1. The Four Knobs

| Policy | Values | Meaning |
|--------|--------|---------|
| InstancingPolicy | NonInstanced / InstancedPerExecution / InstancedPerActor | How many live instances an activation gets |
| NetExecutionPolicy | LocalPredicted / LocalOnly / ServerInitiated / ServerOnly | Where the ability is allowed to run |
| NetSecurityPolicy | ClientOrServer / ServerOnlyExecution / ServerOnlyInitiation | Where RPCs for this ability are accepted from |
| ReplicationPolicy (5.6) | ReplicateNo / ReplicateYes | Whether the ability instance becomes a replicated subobject |

## 2. InstancingPolicy

- **NonInstanced**: the CDO executes. Cheapest, but no per-activation state is possible. Fine for pure-fire-and-forget effects.
- **InstancedPerExecution**: a fresh instance per activation, thrown away at EndAbility. State survives one activation only. Predictive combos are unsupported here (the engine logs it as an invalid configuration when combined with replication).
- **InstancedPerActor**: one instance per owner, reused across activations. Required for: state that spans activations, replicated subobject behavior, and anything the ASC must reference later (tasks that outlive a frame, cached per-owner config).

## 3. NetExecutionPolicy

- **LocalPredicted**: owning client runs immediately under a prediction key; server runs when the activation RPC arrives. The standard for input-driven abilities.
- **LocalOnly**: runs only where activated — cosmetic-only abilities.
- **ServerInitiated**: server activates; the activation replicates and the owning client runs it too. The standard for server-authored events (pickups, AI-driven abilities).
- **ServerOnly**: server only; clients see results through other replicated state.

Both LocalPredicted and ServerInitiated make the ability body execute on **both** the owning client and the server — which is exactly why a well-formed ability can write symmetric state (movement tuning, timers) on both sides without replicating that state.

## 4. ReplicationPolicy (UE 5.6) — the one that bites

`ReplicateNo` puts the ability instance in the spec's `NonReplicatedInstances` list; `ReplicateYes` puts it in `ReplicatedInstances`, and only that list is walked by `UAbilitySystemComponent::ReplicateSubobjects`.

Consequences:

- Custom **Server/Client RPCs declared on the ability class** route through the ASC's subobject channel — with ReplicateNo there is no channel and the call is silently dropped. The 5.6 `UGameplayAbility::IsDataValid` check flags exactly this as an error.
- **NetMulticast on an ability is always meaningless** — abilities are never replicated to simulated proxies — and the validator rejects it outright.
- Valid combination for ability RPCs: **InstancedPerActor + ReplicateYes**. Lyra's base ability defaults to ReplicateNo, so any subclass declaring an RPC must flip it in its constructor.

## 5. Decision Recipes

| Need | Set |
|------|-----|
| Input-driven combat ability, no RPC | InstancedPerActor, LocalPredicted, ReplicateNo |
| Ability with a Server RPC (client sends aiming/target info) | InstancedPerActor, LocalPredicted or ServerInitiated, **ReplicateYes** |
| Server-authored event (pickup, reward) | InstancedPerActor, ServerInitiated, ReplicateNo (unless RPCs) |
| Cosmetic shake/spark | NonInstanced or LocalOnly, ReplicateNo |

## Verification Checklist

- Validator error about RPC + ReplicationPolicy: set `ReplicationPolicy = EGameplayAbilityReplicationPolicy::ReplicateYes` on the class that declares the RPC — all blueprint children inherit it.
- RPC fires on listen-server host but not on dedicated: policy is fine, the subobject channel is missing — same fix.
- Ability behaves differently on client vs server: check NetExecutionPolicy first — you may be running the body on only one side.

## Architecture

The four policies are orthogonal: instancing decides instance lifetime, execution decides where the body runs, security decides who may call RPCs, replication decides whether the instance exists on the network at all. Treating any one as a substitute for another is how silent network bugs are born — especially assuming "it activates on the client, so its RPC will work" (execution ≠ replication).

## Data Flow

```
Client ability instance calls Server RPC
        │
ReplicationPolicy == ReplicateYes ?
        ├─ no  → instance is in NonReplicatedInstances
        │        → no subobject channel → call dropped (validator warned you)
        └─ yes → ASC ReplicateSubobjects replicates the instance
                 → RPC rides the ASC's actor channel to the server
                 → server executes the ability's _Implementation
```

## Key Claims

- [extracted] `EGameplayAbilityReplicationPolicy` is defined at `Plugins/Runtime/GameplayAbilities/Source/GameplayAbilities/Public/Abilities/GameplayAbilityTypes.h` with values ReplicateNo and ReplicateYes.
- [extracted] `FGameplayAbilitySpec::ReplicatedInstances` is defined at `Plugins/Runtime/GameplayAbilities/Source/GameplayAbilities/Public/GameplayAbilitySpec.h` and is the only instance list the ASC replicates as subobjects.
- [extracted] `FGameplayAbilitySpec::NonReplicatedInstances` is defined at `Plugins/Runtime/GameplayAbilities/Source/GameplayAbilities/Public/GameplayAbilitySpec.h` and holds instances that never get a network channel.
- [extracted] `UGameplayAbility` declares networking overrides at `Plugins/Runtime/GameplayAbilities/Source/GameplayAbilities/Public/Abilities/GameplayAbility.h:514` (CallRemoteFunction, GetFunctionCallspace, IsSupportedForNetworking), which is what makes ability RPCs route through the owning ASC.
- [inferred] The 5.6 ReplicationPolicy exists to make ability subobject replication explicit; before it, ability RPC support was implicit and frequently misconfigured.

## Edge Cases

- Flipping ReplicationPolicy on the C++ base class retroactively fixes all blueprint children — the validator reads the class default.
- InstancedPerExecution + ReplicateYes is rejected by the engine as an invalid configuration at activation time.
- Bots' abilities run only on the server regardless of policy — ServerInitiated covers them with no extra work.

## Boundaries

- Ability-level policies are the scope here; GE and attribute replication have their own paths and are out of scope.
- Multicast gameplay events belong to replicated actors or the cue system, never to abilities.

## Evidence

- `EGameplayAbilityInstancingPolicy` defined at `Plugins/Runtime/GameplayAbilities/Source/GameplayAbilities/Public/Abilities/GameplayAbilityTypes.h`
- `EGameplayAbilityNetExecutionPolicy` defined at `Plugins/Runtime/GameplayAbilities/Source/GameplayAbilities/Public/Abilities/GameplayAbilityTypes.h`
- `EGameplayAbilityNetSecurityPolicy` defined at `Plugins/Runtime/GameplayAbilities/Source/GameplayAbilities/Public/Abilities/GameplayAbilityTypes.h`
- `EGameplayAbilityReplicationPolicy` defined at `Plugins/Runtime/GameplayAbilities/Source/GameplayAbilities/Public/Abilities/GameplayAbilityTypes.h`
- `UGameplayAbility::GetReplicationPolicy` defined at `Plugins/Runtime/GameplayAbilities/Source/GameplayAbilities/Public/Abilities/GameplayAbility.h`
- `FGameplayAbilitySpec` defined at `Plugins/Runtime/GameplayAbilities/Source/GameplayAbilities/Public/GameplayAbilitySpec.h`
