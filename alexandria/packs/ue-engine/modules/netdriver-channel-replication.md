---
name: netdriver-channel-replication
description: The replication transport — UNetDriver/UNetConnection, actor channels, and FRepLayout property diffing — the machinery under every Replicated UPROPERTY and RPC.
module: netdriver-channel-replication
---

# NetDriver, Channels & Replication (UE5 Expert)

## Context
- Read before debugging "property didn't replicate", RPC drops, or channel saturation; and before designing anything bandwidth-sensitive.
- Game code sees `Replicated` and `UFUNCTION(Server)`; this doc is what those keywords actually run on.

## 1. The Stack

```
UNetDriver (per world, per connection class)
        │
        ├─ UNetConnection (one per remote client)
        │       │
        │       └─ UChannel subclasses:
        │             UActorChannel — per replicated actor
        │             UControlChannel / UVoiceChannel ...
        │
        ▼
FRepLayout — per-class property diff & serialize
```

The driver owns connections; each connection owns channels; each actor channel owns the lifecycle of one replicated actor on that connection — spawning from the bunch, property updates, RPCs, subobjects, teardown.

## 2. The Property Path

Per frame, the driver iterates dirty actors per connection: `FRepLayout` diffs current properties against the shadow state and serializes only changed fields into the channel bunch. `DOREPLIFETIME` conditions (owner-only, initial-only, simulated...) are evaluated during this diff. **Cost model: change frequency × connections × field size** — a per-frame float on a pawn costs ×N clients; batching into a struct with one RepNotify is often cheaper than five separate properties.

## 3. The RPC Path

`UFUNCTION(Server/Client/NetMulticast)` calls serialize into the actor's channel as an RPC bunch and execute on the far side. Ordering is guaranteed only per channel. RPC drops ("no owning connection", "channel closed") mean the actor/connection state at call time didn't allow it — the call never reaches your `_Implementation`.

## 4. The Actor Channel Lifecycle

Open on first replication of the actor (spawns it remotely from the bunch), property bunches follow, close on destroy. Subobjects ride the owner's channel via `ReplicateSubobjects` — which is exactly the hook GAS uses for ability instances (see [ability-system-component](ability-system-component.md)).

## Verification Checklist

- Property stale on one client: DOREPLIFETIME condition excluded that connection — check owner vs simulated flags.
- RPC never arrives: log the channel state; the actor wasn't net-owned by the calling connection.
- Bandwidth spike: `net.stat` / packet profiling — find the high-frequency dirty property, then push-model or batch it.

## Architecture

Replication is a diffing state-sync system, not a message bus: channels own actor lifecycles, and everything about an actor — spawn, properties, RPCs, subobjects — is framed by that lifecycle. Treating it as "send event to client" (RPC-heavy designs) fights both the ordering model and the bandwidth model.

## Data Flow

```
Property changes on server actor
        │
        ▼
Per-connection replication update
        ├─ FRepLayout diff → changed fields → bunch
        ├─ RPC calls → bunch
        └─ subobject replication (ReplicateSubobjects)
        │
        ▼
UActorChannel per connection → UNetConnection → transport
        │
        ▼
Client: channel applies bunch → properties → OnRep callbacks
```

## Key Claims

- [extracted] `UNetDriver` is defined at `Source/Runtime/Engine/Classes/Engine/NetDriver.h:798` and owns connections and the per-frame replication update.
- [extracted] `UActorChannel` is defined at `Source/Runtime/Engine/Classes/Engine/ActorChannel.h:77` and owns one replicated actor's lifecycle on one connection.
- [extracted] `UNetConnection` is defined at `Source/Runtime/Engine/Classes/Engine/NetConnection.h:283` and owns the channel set for one remote client.
- [extracted] `FRepLayout` is defined at `Source/Runtime/Engine/Public/Net/RepLayout.h:125` and implements per-class property diffing and serialization.
- [inferred] Replication bandwidth is a per-connection product, so cost decisions must be made against the connection count, not the actor count.

## Edge Cases

- Initial replication of a level's actors all lands at once — join-in-progress spikes are channel-open storms, mitigated by relevancy.
- Dormancy pauses property updates for an actor channel; waking requires explicit flush semantics.
- Push-model replication marks fields dirty explicitly and skips the per-frame diff for them.

## Boundaries

- The transport layer stops at bunches and channels; Iris replication is a parallel newer stack with its own docs.
- HTTP/online-subsystem traffic does not ride these channels at all.

## Evidence

- `UNetDriver` defined at `Source/Runtime/Engine/Classes/Engine/NetDriver.h:798`
- `UActorChannel` defined at `Source/Runtime/Engine/Classes/Engine/ActorChannel.h:77`
- `UNetConnection` defined at `Source/Runtime/Engine/Classes/Engine/NetConnection.h:283`
- `FRepLayout` defined at `Source/Runtime/Engine/Public/Net/RepLayout.h:125`
