---
name: gas-asc-loose-gameplay-tags
description: UAbilitySystemComponent loose gameplay tags — non-replicated, refcounted tag storage. Covers the aggregated tag container pitfall and replicated input parity.
domain: gas
---

# GAS: ASC Loose Gameplay Tags (UE5 Expert)

## Context

- Loose gameplay tags are tag storage on `UAbilitySystemComponent` that is not backed by a GameplayEffect, added directly from code.
- Trigger when you need a refcounted on/off gate on the ASC (input windows, state markers) without authoring a GameplayEffect.
- Verified against UE 5.6 source, not assumed from API names.

## 1. Loose Tags Are NOT Replicated

`AddLooseGameplayTag` / `RemoveLooseGameplayTag` never replicate. The engine header states it directly: "Tags added this way are not replicated! Use the 'Replicated' versions of these functions if replication is needed." The replicating variants are `AddReplicatedLooseGameplayTag` / `RemoveReplicatedLooseGameplayTag`, which write to a separate replicated container.

Consequence: server and clients each maintain their own loose-tag state. Both sides must run the same add/remove calls (e.g. driven by anim notifies that fire on both sides) — do not expect the server to see a client's loose tags.

## 2. Loose Tags Are Refcounted

Each add/remove adjusts a per-tag count via `UpdateTagMap`; the tag disappears when its count reaches zero, and removes below zero clamp harmlessly. Overlapping openers are therefore safe: two systems can hold the same tag, and the first one to close does not shut the gate on the second. This makes loose tags a zero-boilerplate refcounted window — no custom `TMap<FGameplayTag, int32>` bookkeeping needed.

## 3. There Is No Loose-Only Public Query

All tag sources — loose tags, replicated loose tags, and GameplayEffect-granted tags — flow into the same `FGameplayTagCountContainer`. Two queries differ only in strictness:

- `HasMatchingGameplayTag(Tag)` checks the aggregated count map, which also counts **parent tags** — a granted `A.B` makes `HasMatchingGameplayTag(A)` true.
- `GetOwnedGameplayTags()` returns the explicit tag container — no parent expansion, but **still includes GE-granted tags**.

So a GE granting your gate tag will hold a "loose-tag gate" open under either query. True loose-only checks require tracking the tags yourself.

## Architecture

```
UAbilitySystemComponent
├─ loose tags        → AddLooseGameplayTag / RemoveLooseGameplayTag (local only, refcounted)
├─ replicated loose  → AddReplicatedLooseGameplayTag (separate replicated container)
├─ GE-granted tags   → gameplay effects add/remove via the active effects container
└─ all three land in FGameplayTagCountContainer
     ├─ HasMatchingGameplayTag → aggregated count map (parents expanded)
     └─ GetOwnedGameplayTags   → explicit container (no parents, GE tags included)
```

## Data Flow

```
Game code calls AddLooseGameplayTag(Tag) / RemoveLooseGameplayTag(Tag)
  → UAbilitySystemComponent::UpdateTagMap(Tag, ±Count)
    → FGameplayTagCountContainer (shared with GE-granted + replicated tags)
Query side:
  → HasMatchingGameplayTag: aggregated count map, parents included
  → GetOwnedGameplayTags: explicit container, no parents, GE tags included
```

## 4. Replicated Input Parity (WaitInputPress)

ASC input-tag funnels (e.g. Lyra's `AbilityInputTagPressed`) run only where the input physically exists — the locally controlled client. `UAbilityTask_WaitInputPress` bridges the gap: on a predicting client it calls `ServerSetReplicatedEvent(EAbilityGenericReplicatedEvent::InputPressed, ...)`, so the press callback fires on the server too. Any client-side input buffer that the server also needs (combo advancement, server-side validation) must be fed from this replicated callback, not from the client-only funnel.

## Key Claims

- [extracted] `AddLooseGameplayTag` defined at `Engine/Plugins/Runtime/GameplayAbilities/Source/GameplayAbilities/Public/AbilitySystemComponent.h:654` does not replicate; the header comment says so verbatim.
- [extracted] `AddReplicatedLooseGameplayTag` defined at `Engine/Plugins/Runtime/GameplayAbilities/Source/GameplayAbilities/Public/AbilitySystemComponent.h:694` is the replicating variant.
- [extracted] `GetOwnedGameplayTags` defined at `Engine/Plugins/Runtime/GameplayAbilities/Source/GameplayAbilities/Public/AbilitySystemComponent.h:597` returns the explicit tag container without parent expansion.
- [inferred] GameplayEffect-granted tags land in the same `FGameplayTagCountContainer` as loose tags, so neither query can exclude them.
- [inferred] `UAbilityTask_WaitInputPress` replicates presses via `ServerSetReplicatedEvent`, making its callback the server-parity feed for client-side input buffers.

## Edge Cases

- Removing a loose tag that was never added clamps at zero — safe as a defensive cleanup in `EndAbility`.
- A tag held open by a GameplayEffect reads as "open" under both queries; gate tags must never double as GE-granted tags.
- Server and client see different loose-tag state if only one side runs the add/remove calls; both sides need the same driver (anim notifies fire on both).

## Boundaries

- This document does **not** cover `IGameplayTagAssetInterface` tag queries on plain actors — see the gameplay-tag-integration doc.
- This document does **not** cover ability-granted tags (AbilityTags / activation-owned tags), which follow the ability lifecycle instead.
- Loose tags are per-ASC storage; this document does **not** cover global or level-script tag usage.

## Evidence

- `AddLooseGameplayTag` defined at `Engine/Plugins/Runtime/GameplayAbilities/Source/GameplayAbilities/Public/AbilitySystemComponent.h:654`
- `RemoveLooseGameplayTag` defined at `Engine/Plugins/Runtime/GameplayAbilities/Source/GameplayAbilities/Public/AbilitySystemComponent.h:664`
- `AddReplicatedLooseGameplayTag` defined at `Engine/Plugins/Runtime/GameplayAbilities/Source/GameplayAbilities/Public/AbilitySystemComponent.h:694`
- `GetOwnedGameplayTags` defined at `Engine/Plugins/Runtime/GameplayAbilities/Source/GameplayAbilities/Public/AbilitySystemComponent.h:597`
- `GetTagCount` defined at `Engine/Plugins/Runtime/GameplayAbilities/Source/GameplayAbilities/Public/AbilitySystemComponent.h:609`
