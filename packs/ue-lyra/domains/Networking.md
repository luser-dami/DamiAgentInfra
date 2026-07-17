---
domain: Networking
tags: [networking, replication, prediction]
source: manual
---

# Networking

Lyra's multiplayer is built on Unreal's actor replication, the Gameplay
Ability System's local prediction, and a custom Replication Graph that keeps
replication cost bounded on large maps. Authority stays server-side; clients
predict and reconcile.

## Context

- **Scope:** domain (spans System, Player, AbilitySystem, Equipment)
- **Pillars:** actor replication · GAS prediction · replication graph
- **Authority model:** server-authoritative; clients predict and reconcile
- **Consumers:** every gameplay system that touches replicated state

## Data Flow

How a local action reaches every player:

```
Client input
   │
   ▼
ULyraGameplayAbility (predicted activation on the client)
   │   server RPC
   ▼
Server: ULyraAbilitySystemComponent (authoritative execution)
   │
   ▼
Replicated state (attributes, equipment list, weapon state)
   │
   ▼
ULyraReplicationGraph (decides who needs what, at which rate)
   │
   ▼
Other clients apply state; the predicting client reconciles
```

## Replication Graph Architecture

`ULyraReplicationGraph` replaces the default "every actor vs every connection"
check with node-based routing: a **spatial grid node** buckets actors by
location so only nearby actors replicate to a connection, an **always-relevant
node** covers actors everyone must see, and a **player-state frequency
limiter** node throttles low-priority player state updates. Class-level
replication defaults are centralised in code
(`InitGlobalActorClassSettings`), so tuning happens in one place.

## Prediction Flow

Gameplay abilities activate locally before the server answers, so firing and
movement feel instant; predicted GameplayEffects (like cooldowns) apply
immediately and are reconciled when the server's authoritative version
arrives. Mispredictions roll back through GAS's prediction key mechanism.
Server-confirmed actions (equip, pickup) stay authority-only by design.

## Key Claims

- [extracted] `ULyraReplicationGraph` is defined at `Source/LyraGame/System/LyraReplicationGraph.h:15` and routes replication through spatial and always-relevant nodes.
- [extracted] `ALyraPlayerController` is defined at `Source/LyraGame/Player/LyraPlayerController.h:33` and anchors the client's connection-side state.
- [inferred] Server authority is the invariant: anything gameplay-meaningful is decided server-side, while prediction only ever *anticipates* presentation.

## Boundaries

- This document does **not** cover Unreal's replication system itself — only Lyra's usage of it.
- It does **not** cover replication of cosmetics (pawns' appearance) in detail.
- It does **not** document network performance budgets or bandwidth numbers.

## Evidence

- `ULyraReplicationGraph` defined at `Source/LyraGame/System/LyraReplicationGraph.h:15`
- `ALyraPlayerController` defined at `Source/LyraGame/Player/LyraPlayerController.h:33`
