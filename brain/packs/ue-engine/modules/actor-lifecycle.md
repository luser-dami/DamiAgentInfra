---
name: actor-lifecycle
description: The AActor lifecycle — spawn, construction, BeginPlay/EndPlay ordering, destruction, and the network timing rules that decide what is safe to call when.
module: actor-lifecycle
---

# Actor Lifecycle (UE5 Expert)

## Context
- Mandatory before putting logic in constructors, BeginPlay, or network RPCs that touch other actors.
- Most "works in PIE, breaks on dedicated" bugs are lifecycle-order bugs, not replication bugs.

## 1. Spawn Pipeline (runtime spawn)

```
UWorld::SpawnActor
  │
  ▼
Constructor (CDO copy) — components created here via CreateDefaultSubobject
  │
  ▼
PostInitializeComponents — components registered, InitializeComponent runs
  │
  ▼
OnConstruction (build script equivalent)
  │
  ▼
BeginPlay — deferred until the world actually begins play
```

Placed (level) actors run the same path at load time: constructor → PostInitializeComponents → OnConstruction → world begin play → BeginPlay.

## 2. Construction Rules That Bite

- The constructor also builds the **CDO at module startup** — before the game instance, the world, and ini-registered systems (GameplayTags included) necessarily exist. Anything beyond trivial member defaults belongs in PostInitializeComponents or BeginPlay, never the constructor.
- `CreateDefaultSubobject` in the constructor is safe; `NewObject`, world queries, and `RequestGameplayTag`-style lookups are not.
- Component `InitializeComponent` runs before the owner's BeginPlay; component BeginPlay runs with the owner's.

## 3. EndPlay and Destruction

`AActor::EndPlay` fires with a reason (Destroyed, LevelTransition, EndPlayInEditor, RemovedFromWorld). `AActor::Destroy` schedules teardown; the actor is not gone until GC — pointers stay valid but `IsValid`/`IsPendingKill` semantics apply. Timer and delegate unbinding belongs in EndPlay, not the destructor.

## 4. Network Timing Rules

- Server spawn → replication → client constructs the actor from the bunch and runs BeginPlay **when the channel opens**, possibly before all properties arrive — BeginPlay on a client cannot assume replicated state is final.
- Possession is its own step: a client must not call Server RPCs on a pawn until its PlayerController has finished possessing it, or the call drops with "no owning connection".
- Role changes (net promote/demote) do not re-run BeginPlay.

## Verification Checklist

- Crash or empty tag/container in a constructor: move the lookup to PostInitializeComponents.
- Client BeginPlay sees stale/default values: gate on a replicated init tag or OnRep instead of BeginPlay.
- RPC "no owning connection": you called before possession completed.

## Architecture

The lifecycle is a contract about what exists at each stage: ctor = self defaults only; PostInitializeComponents = components resolved; OnConstruction = editor-time rebuilds; BeginPlay = world present; possession = controller linkage; first replicated bunch = client-visible state. Writing logic against the wrong stage is the single most common Unreal initialization bug.

## Data Flow

```
Server: SpawnActor → components → OnConstruction → BeginPlay → possess
   │
   └─ replication bunch ────────────────┐
                                        ▼
Client: channel opens → construct from archetype → BeginPlay
        → properties continue arriving → OnRep callbacks
        → possession completes → client may now call Server RPCs
```

## Key Claims

- [extracted] `AActor::PostInitializeComponents` is defined at `Source/Runtime/Engine/Private/Actor.cpp:6531` and runs before BeginPlay on both placed and spawned actors.
- [extracted] `AActor::BeginPlay` is defined at `Source/Runtime/Engine/Private/Actor.cpp:4737` and is deferred until the world begins play, not the spawn call.
- [extracted] `AActor::EndPlay` is defined at `Source/Runtime/Engine/Private/Actor.cpp:3207` and carries an explicit reason enum that teardown logic should branch on.
- [inferred] Because the constructor also runs for the CDO at startup, constructor code must be treatable as static-init: no world, no game systems, no lookups.

## Edge Cases

- Sequenced/spawn-deferred actors (BeginDeferredActorSpawn) intentionally delay PostInitializeComponents — finish them with FinishSpawningActor.
- EndPlay with LevelTransition fires without Destroy being called by game code — cleanup keyed only on Destroy leaks.
- Child actor components run the same lifecycle nested inside the parent's.

## Boundaries

- Actor/component lifecycle is the scope here; pawn input setup and ASC init have their own sequencing on top (hero/pawn-extension components in Lyra).
- Garbage collection details live in the UObject/reflection doc; only the Destroy-facing half is here.

## Evidence

- `AActor` defined at `Source/Runtime/Engine/Classes/GameFramework/Actor.h:253`
- `AActor::PostInitializeComponents` defined at `Source/Runtime/Engine/Private/Actor.cpp:6531`
- `AActor::BeginPlay` defined at `Source/Runtime/Engine/Private/Actor.cpp:4737`
- `AActor::EndPlay` defined at `Source/Runtime/Engine/Private/Actor.cpp:3207`
