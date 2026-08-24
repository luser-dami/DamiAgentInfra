---
name: garbage-collection
description: Unreal's garbage collection — mark-and-sweep over the global UObject array, the reachability rules that decide what lives, the destroy sequence, and the cost model behind GC hitches.
module: garbage-collection
---

# Garbage Collection Internals (UE5 Expert)

## Context
- Read before storing UObject pointers anywhere, debugging "destroyed but not null" crashes, or profiling periodic frame spikes.
- Every UObject bug that looks random is one of the reachability rules below being violated — the collector itself is rarely wrong.

## 1. The Model

All UObjects live in one global `FUObjectArray`. GC is **mark-and-sweep** over that array:

```
Root set (AddToRoot'd objects + engine roots)
        │
        ▼
Mark: FReferenceCollector walks UPROPERTY edges via reflection
        │
        ▼
Sweep: unmarked objects → BeginDestroy → FinishDestroy → memory freed
```

No reference counting, no manual registration — the reflection records built at startup are the graph. An object lives exactly as long as some UPROPERTY chain from a root reaches it.

## 2. The Reachability Rules (the part that bites)

| Holder | Keeps alive | Auto-cleared | Use for |
|--------|-------------|--------------|---------|
| `UPROPERTY` member | yes | yes (nulled) | default ownership |
| Raw `T*` (non-UPROPERTY) | **no** | no | never for ownership |
| `TWeakObjectPtr` | no | yes | observers, caches |
| `TStrongObjectPtr` | yes | yes | owning without a UPROPERTY context |
| `AddToRoot()` | yes (pinned) | no | singletons, registries — leaks if forgotten |

The two symmetric failure modes: a raw pointer dangles (object collected under you), and a forgotten root/strong ref leaks (object never collected).

## 3. The Destroy Sequence

Unreachable does not mean instantly gone: the sweep calls `BeginDestroy` (virtual, object still fully valid — last chance for world-touching cleanup), polls readiness, then `FinishDestroy`, then frees memory over subsequent frames (incremental purge). Consequences:

- Delegate/timer unbinding belongs in BeginDestroy — or EndPlay for actors, which runs earlier and safer.
- The destructor is too late for anything touching the world or other UObjects.
- "Pending kill" is a real, observable state — `IsValid` returns false for it even though the memory is still there.

## 4. The Cost Model

GC cost scales with the **reachable graph walk**, not raw object count: every live UPROPERTY edge is visited. Big spikes come from mass invalidation (level transitions, asset dumps), and are smoothed by incremental purging and GC clustering (large object groups processed as units). Practical responses: pool high-churn objects instead of NewObject-ing per event, don't keep giant alive-by-accident graphs (a rooted manager holding the world), and measure with `stat GC` before tuning cvars.

## Verification Checklist

- Crash on dangling pointer: find the non-UPROPERTY holder — make it weak or UPROPERTY.
- Memory never frees: find the accidental root — strong refs, AddToRoot, or a UPROPERTY chain from a long-lived owner.
- Periodic hitch at load boundaries: `stat GC` and cluster behavior — usually one giant graph walk, not many small ones.

## Architecture

The collector trades determinism for zero-annotation ownership: you never declare lifetimes, only references, and the reflection system derives the rest. That trade is why constructor and pointer discipline matter so much in Unreal — the GC can only protect what it can see, and it can only see UPROPERTYs and roots.

## Data Flow

```
NewObject → FUObjectArray slot
        │
        ▼
UPROPERTY links form the reachable graph
        │
        ▼
CollectGarbage: mark from roots → unmarked flagged pending kill
        │
        ▼
BeginDestroy → FinishDestroy → incremental purge frees memory
        │
        ▼
Weak refs auto-clear; UPROPERTY refs nulled
```

## Key Claims

- [extracted] `CollectGarbage` is defined at `Source/Runtime/CoreUObject/Public/UObject/UObjectGlobals.h:912` and is the entry point for mark-and-sweep collection.
- [extracted] `FUObjectArray` is defined at `Source/Runtime/CoreUObject/Public/UObject/UObjectArray.h:885` and is the global array every UObject lives in.
- [extracted] `UObjectBaseUtility::AddToRoot` is defined at `Source/Runtime/CoreUObject/Public/UObject/UObjectBaseUtility.h:206` and pins an object into the GC root set.
- [extracted] `UObject::BeginDestroy` is defined at `Source/Runtime/CoreUObject/Public/UObject/Object.h:368` and is the last valid-object cleanup point before teardown.
- [inferred] GC hitches scale with reachable-graph size, so accidental retention (rooted objects holding large graphs) is the dominant real-world cost driver.

## Edge Cases

- Actors are UObjects but their teardown usually starts with `Destroy` + EndPlay — actor cleanup belongs there, not in GC hooks.
- Objects in disregard-for-GC packages skip walking entirely — legitimate for permanent data, fatal if gameplay objects leak in.
- Async loading holds objects alive through load handles; GC during a load is coordinated, not instantaneous.

## Boundaries

- The collector covers UObjects only; non-UObject memory (FMemory allocations, third-party) follows the allocation layer's rules in [memory-allocation](memory-allocation.md).
- Reachability policy for networking (who replicates what) is unrelated to GC reachability — a replicated object is not automatically rooted.

## Evidence

- `CollectGarbage` defined at `Source/Runtime/CoreUObject/Public/UObject/UObjectGlobals.h:912`
- `FUObjectArray` defined at `Source/Runtime/CoreUObject/Public/UObject/UObjectArray.h:885`
- `UObjectBaseUtility::AddToRoot` defined at `Source/Runtime/CoreUObject/Public/UObject/UObjectBaseUtility.h:206`
- `UObject::BeginDestroy` defined at `Source/Runtime/CoreUObject/Public/UObject/Object.h:368`
