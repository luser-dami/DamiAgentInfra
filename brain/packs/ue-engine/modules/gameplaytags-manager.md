---
name: gameplaytags-manager
description: UGameplayTagsManager internals — tag registration sources (native vs ini vs table), the startup sequence that makes constructor-time lookups fatal, and tag networking.
module: gameplaytags-manager
---

# GameplayTags Manager Internals (UE5 Expert)

## Context
- Read before adding tag registration code, debugging "tag not found" at startup, or choosing where a tag should live.
- The constructor-time `RequestGameplayTag` crash every project hits once is a direct consequence of the registration order below.

## 1. Where Tags Come From

Three registration sources, one namespace:

| Source | When registered | Use for |
|--------|-----------------|---------|
| Native (`UE_DEFINE_GAMEPLAY_TAG`) | Module startup, before CDOs finish | Any tag referenced from C++ |
| ini GameplayTagList | GameplayTagsSettings load during engine init | Asset-only tags (input, messages, cues) |
| DataTable (GameplayTagTableList) | After ini | Designer-bulk tag sets |

`FGameplayTag` itself is just a `FName` wrapper; identity is name equality, nothing more.

## 2. The Startup Sequence (why ctor lookups die)

```
Module startup: native tags register first (FNativeGameplayTag static instances)
        │
        ▼
CDOs construct — constructor code runs NOW
        │
        ▼
GameplayTagsSettings (ini) loads → ini/table tags register
        │
        ▼
Manager fully initialized → editor/game boot continues
```

A `FGameplayTag::RequestGameplayTag` inside a constructor runs in the middle band: native tags exist, ini tags do not — the request either fatals or returns an invalid tag depending on flags. The rule that falls out: code-referenced tags are always native (`UE_DEFINE_GAMEPLAY_TAG`), and UPROPERTY defaults are set with in-class initializers, never constructor bodies.

## 3. Containers and Queries

`FGameplayTagContainer` is a sorted TArray<FGameplayTag> with set operations. `HasTag` matches the tag itself; `HasTagExact` ignores parent expansion. `MatchesTag` treats `A.B` as matching a query for `A` — the hierarchy is purely name-prefix based, there is no type system behind it.

## 4. Networking

With FastReplication enabled, tags travel as indices into a shared tag table maintained per channel (GameplayTagNetIndex); containers serialize to compact index arrays. Native + ini tags must agree on both sides or indices diverge — mismatched tag tables are a classic source of "different tag on client" bugs in source builds vs packaged builds.

## Verification Checklist

- Startup fatal mentioning a tag: find the RequestGameplayTag in a ctor/static-init path and nativize the tag.
- Tag works in editor, missing in packaged build: the ini file or table was not staged — check packaging's ini/table inclusion.
- Container query always false: you wanted HasTag (parents included) but called HasTagExact, or vice versa.

## Architecture

The manager is a singleton registry: name → metadata (dev comment, source, net index). It owns no behavior — it exists so that a flat name namespace can be shared by code, assets, and the network without string comparisons at runtime. Treating tags as an enum-with-hierarchy is correct; expecting more (per-tag logic, ownership) leads to reimplementing it badly elsewhere.

## Data Flow

```
Native define (module load) ─┐
ini GameplayTagList ─────────┼→ UGameplayTagsManager registry
DataTable list ──────────────┘         │
                                       ▼
FGameplayTag (FName) ←──────── RequestGameplayTag (runtime lookup)
                                       │
        ┌──────────────────────────────┼─────────────────┐
        ▼                              ▼                 ▼
   ASC tag containers          GE/ability gating     FastReplication indices
```

## Key Claims

- [extracted] `UGameplayTagsManager` is defined at `Source/Runtime/GameplayTags/Classes/GameplayTagsManager.h:329` and is the singleton registry for all tag sources.
- [extracted] `FGameplayTag` is defined at `Source/Runtime/GameplayTags/Classes/GameplayTagContainer.h:53` and is an FName-based identity with no attached behavior.
- [extracted] `UE_DEFINE_GAMEPLAY_TAG` is defined at `Source/Runtime/GameplayTags/Public/NativeGameplayTags.h:41` and registers the tag at module load, before CDO construction completes.
- [inferred] Constructor-time RequestGameplayTag failures are an initialization-order artifact: native tags register before CDOs, ini tags after, so ctor lookups sit in the gap.

## Edge Cases

- Duplicate registration of the same tag string across native and ini sources creates ambiguity in comments/net handling — one tag, one home.
- Renaming a tag breaks every serialized reference; redirects belong in the tag redirect settings, not find-replace.
- Loose ASC tags and ini tags share the namespace but not the lifetime model.

## Boundaries

- The registry covers names and metadata only; tag-driven behavior (ability gating, cues) belongs to the systems that consume tags.
- GameplayTagQueries are a separate expression layer on top of containers.

## Evidence

- `UGameplayTagsManager` defined at `Source/Runtime/GameplayTags/Classes/GameplayTagsManager.h:329`
- `FGameplayTag` defined at `Source/Runtime/GameplayTags/Classes/GameplayTagContainer.h:53`
- `FGameplayTagContainer` defined at `Source/Runtime/GameplayTags/Classes/GameplayTagContainer.h:259`
- `UE_DEFINE_GAMEPLAY_TAG` defined at `Source/Runtime/GameplayTags/Public/NativeGameplayTags.h:41`
