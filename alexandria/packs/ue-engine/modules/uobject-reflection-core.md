---
name: uobject-reflection-core
description: The UObject core — reflection macros to UClass registration, the Class Default Object, outer/name identity, GC reachability, and the construction rules these impose on your code.
module: uobject-reflection-core
---

# UObject & Reflection Core (UE5 Expert)

## Context
- Read before writing any constructor, touching raw UObject pointers, or wondering why a UPROPERTY edit "didn't take" on existing instances.
- Everything in Unreal is negotiated with this layer; fighting it (raw pointers, ctor side effects) produces the crashes people blame on "Unreal being flaky".

## 1. From Macro to Registered Type

```
UCLASS/USTRUCT/UENUM + UPROPERTY/UFUNCTION
        │
        ▼
UnrealHeaderTool → *.generated.h (reflection records)
        │
        ▼
Module startup → UClass/UProperty/UFunction objects registered
        │
        ▼
CDO constructed per class — the archetype every instance copies
```

`UObject::StaticClass()` returns the type object; `IsA`/`Cast` walk the UClass chain. Blueprint classes are UClasses too — a blueprint "class default" edit is a CDO edit on a generated class.

## 2. The Class Default Object

Every UClass has one CDO, built by running the C++ constructor at startup and then applying serialized defaults. Consequences:

- The constructor is static-init code: no world, no game systems, no ini-registered lookups (they may not exist yet — the classic startup fatal).
- Editing a UPROPERTY default changes the CDO; instances that never overrode that property follow the new value, overridden instances keep theirs.
- `GetDefault<T>()` is the read API; mutating the CDO at runtime is a global change, not a per-instance one.

## 3. Identity: Name, Outer, Flags

A UObject is named and owned by its Outer (package → object hierarchy), forming the path `/Game/Foo/Bar.Baz`. Subobjects of an actor default to the actor as outer. The outer chain is what makes subobject replication addressable and what GC walks.

## 4. GC Reachability

- `UPROPERTY` references keep targets alive and are nulled on destruction.
- Raw C++ pointers are invisible to GC — the object dies under you.
- Non-UPROPERTY holders use `TWeakObjectPtr` (observing) or `AddToRoot` (owning, rare).
- `TObjectPtr` is the UPROPERTY member form in UE5; behavior matches raw pointers plus editor/lazy-load semantics.

## 5. Construction Recipes

| Need | Use |
|------|-----|
| Component on an actor | `CreateDefaultSubobject` (ctor only) |
| Plain UObject at runtime | `NewObject<T>(Outer)` |
| Archetype/duplicate | `StaticDuplicateObject` |
| Load an asset | `StaticLoadObject` / soft refs + async loading |

## Verification Checklist

- Startup fatal in a constructor: it ran for the CDO before your system existed — move the work to PostInitProperties/BeginPlay.
- Property reset "ignored": the instance overrides the property; CDO edits do not touch overrides.
- Random GC crash: a raw pointer escaped a UPROPERTY — audit the class's pointer members.

## Architecture

Reflection is not metadata bolted onto objects — it IS the object system: serialization, GC, networking, and the editor all drive off the same UProperty records. Any side channel (raw pointers, manual type tags, constructor side effects) is invisible to all four of those systems at once.

## Data Flow

```
Header with macros → UHT → generated reflection records
        │
        ▼
Startup: UClass registered → CDO constructed (ctor + serialized defaults)
        │
        ▼
Runtime instance = CDO copy + instance overrides
        │
        ▼
GC marks from roots → UPROPERTY graph keeps reachable objects alive
```

## Key Claims

- [extracted] `UObject` is defined at `Source/Runtime/CoreUObject/Public/UObject/Object.h:93` and every engine object derives from it.
- [extracted] `UClass` is defined at `Source/Runtime/CoreUObject/Public/UObject/Class.h:2981` and is the reflection type object for classes.
- [extracted] `GetDefault` is defined at `Source/Runtime/CoreUObject/Public/UObject/Class.h:4297` and is the supported read path to a class's CDO.
- [inferred] Because the CDO is built at module startup, constructor code executes in a static-init context where gameplay systems are not guaranteed to exist.

## Edge Cases

- Blueprint subclasses re-run the parent constructor when recompiled — ctor side effects must be idempotent.
- `RF_ArchetypeObject`/`RF_ClassDefaultObject` flags distinguish archetypes from live instances; editing tools must check them.
- Duplicating actors in the editor copies instance overrides, not just CDO state.

## Boundaries

- The object core is the scope here; UPROPERTY/UFUNCTION specifier semantics and metadata have their own feature docs.
- Serialization formats (FArchive, SaveGame) are downstream consumers of the same records, covered separately.

## Evidence

- `UObject` defined at `Source/Runtime/CoreUObject/Public/UObject/Object.h:93`
- `UClass` defined at `Source/Runtime/CoreUObject/Public/UObject/Class.h:2981`
- `GetDefault` defined at `Source/Runtime/CoreUObject/Public/UObject/Class.h:4297`
