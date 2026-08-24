---
name: animation-evaluation-pipeline
description: The animation runtime pipeline — from USkeletalMeshComponent tick through UAnimInstance and the AnimGraph to the final pose, including the game-thread/worker split and montage injection.
module: animation-evaluation-pipeline
---

# Animation Evaluation Pipeline (UE5 Expert)

## Context
- Read before writing anim instance code, debugging one-frame-late poses, or fighting animation/network replication.
- The pipeline is designed around one rule: game logic asks for animation, the graph produces it — never the reverse.

## 1. The Chain

```
USkeletalMeshComponent::TickAnimation
        │
        ▼
UAnimInstance (game thread): NativeUpdateAnimation
        ├─ read owning pawn state (CMC velocity, ASC tags)
        └─ write graph-facing properties
        │
        ▼
FAnimInstanceProxy (worker thread): state machines + AnimGraph nodes evaluate
        │
        ▼
Pose → bone transforms → skinning
```

The proxy owns the evaluation data so the graph can run off the game thread; anything the graph needs must be copied into it during the update phase.

## 2. The Thread Split (why your variable is one frame late)

Game-thread `NativeUpdateAnimation` computes inputs; worker-thread evaluation consumes them. Values set on the anim instance outside the update window race with evaluation — the proxy pattern exists so inputs are snapshotted, not shared. Direct node property writes from arbitrary game code are the classic cause of flicker and one-frame lag.

## 3. State Sources

- **State machines** pick the locomotion state from the inputs you copied into the proxy.
- **Gameplay tags** (Lyra-style) drive state selection deterministically from ASC state — the ASC mirrors tags, the graph reads them.
- **Montages** inject one-shot full-body/layered animation over the state machine and report progress via delegates and notifies — the gameplay-ability-friendly path for attacks and dashes.

## 4. Networking Reality

Animation state is simulated locally on every client from replicated movement/ability state; only root motion is server-authoritative. Never replicate pose or per-node state — replicate the *inputs* (movement, montage start via ability RPCs/cues) and let each side evaluate the same graph.

## Verification Checklist

- Pose lags one frame behind logic: you wrote anim inputs outside the update window — move them into NativeUpdateAnimation.
- Montage plays on server, not on clients: montage start wasn't replicated — route it through the ability system's montage replication or a cue.
- Anim cost spike: reduce active state machines and node count before blaming threads; check `a.AnimNodeStats` style counters.

## Architecture

The pipeline is a pull system with a strict interface: the anim instance pulls game state into the proxy, the graph pulls proxy state into poses. Game code that pushes into the graph directly (node references, ad-hoc property pokes) breaks threading and determinism at once.

## Data Flow

```
Game state (CMC, ASC tags, ability montage requests)
        │
        ▼ game thread
UAnimInstance::NativeUpdateAnimation → snapshot into FAnimInstanceProxy
        │
        ▼ worker thread
State machines + blend nodes + montage layer evaluate
        │
        ▼
Final pose → skeletal mesh skinning
```

## Key Claims

- [extracted] `UAnimInstance` is defined at `Source/Runtime/Engine/Classes/Animation/AnimInstance.h:352` and is the game-thread face of the animation pipeline.
- [extracted] `USkeletalMeshComponent` is defined at `Source/Runtime/Engine/Classes/Components/SkeletalMeshComponent.h:314` and owns the skeletal pose the pipeline writes into.
- [extracted] `USkeletalMeshComponent::TickAnimation` is defined at `Source/Runtime/Engine/Private/Components/SkeletalMeshComponent.cpp:1646` and drives the per-frame animation update.
- [inferred] The game-thread/worker split makes input snapshotting mandatory: values consumed by the graph must be copied during the update phase or they race.

## Edge Cases

- Leader-pose-component setups (weapon/cape following body) evaluate in dependency order — cyclic dependencies silently produce stale poses.
- Root motion sources (ability-driven) compose with montage root motion; conflicts resolve inside the CMC, not the graph.
- Dedicated servers skip most evaluation by default — logic must never depend on server-side poses.

## Boundaries

- The pipeline ends at pose production; IK, physics bodies, and control rig run as post-process stages on that pose.
- Sequencer/level-driven animation follows a different authority model and is out of scope.

## Evidence

- `UAnimInstance` defined at `Source/Runtime/Engine/Classes/Animation/AnimInstance.h:352`
- `USkeletalMeshComponent` defined at `Source/Runtime/Engine/Classes/Components/SkeletalMeshComponent.h:314`
- `USkeletalMeshComponent::TickAnimation` defined at `Source/Runtime/Engine/Private/Components/SkeletalMeshComponent.cpp:1646`
