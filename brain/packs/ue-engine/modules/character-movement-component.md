---
name: character-movement-component
description: UCharacterMovementComponent internals — frame pipeline, the three-layer prediction model, saved-move lifecycle, and the extension-point tiers for custom movement.
module: character-movement-component
---

# CharacterMovementComponent Internals (UE5 Expert)

## Context
- Read before subclassing the CMC, chasing rubberband/desync, or touching prediction state.
- The how-to for building a custom movement mode is `features/custom-character-movement.md`; this doc is the machine those hooks plug into.

## 1. Frame Pipeline (game thread)

`UCharacterMovementComponent::TickComponent` splits by net role, then converges on one physics path:

- **AutonomousProxy (owning client):** `MoveAutonomous` → `PerformMovement` locally, then `ReplicateMoveToServer` buffers the frame into a saved move and sends it (`ServerMove` / packed moves).
- **Authority (server):** `PerformMovement` for locally controlled pawns, plus re-simulation of each received client move.
- **SimulatedProxy (other viewers):** `SimulatedTick` only — no physics, pose comes from replicated velocity + smoothing.

Inside `PerformMovement`: `UpdateCharacterStateBeforeMovement` → `StartNewPhysics` dispatches on MovementMode → `PhysWalking` / `PhysFalling` / `PhysCustom` → `CalcVelocity` (input accel + friction + braking, clamped by `GetMaxSpeed`) → `MoveAlongFloor` → `SafeMoveUpdatedComponent` (capsule collision) → `SlideAlongSurface` / `StepUp` → `FindFloor` → `UpdateComponentVelocity`.

## 2. Network Model: three layers, one invariant

1. The owning client never waits for the server — it predicts locally and keeps a `SavedMoves` history of unacknowledged frames.
2. The server re-simulates the same inputs with the same physics code.
3. On mismatch the server sends `ClientAdjustPosition`; the client rewinds to the server state and replays unacked moves (`FSavedMove_Character::PrepMoveFor`), while `SmoothCorrection` hides the pop visually.

**Invariant:** any state that changes physics must reach the simulating side deterministically — either (a) carried per-move through a `FSavedMove_Character` subclass (`SetMoveFor` → `GetCompressedFlags` → `UpdateFromCompressedFlags` → `PrepMoveFor`), or (b) applied symmetrically on both sides at the same logical time (e.g. a gameplay ability that activates on client and server). State written only on the client becomes a correction storm, because the server keeps simulating different physics and pulling the client back.

## 3. Extension-Point Tiers (cheapest correct one wins)

| Tier | Mechanism | Use for |
|------|-----------|---------|
| Parameter change | Override `GetMaxSpeed` / `GetMaxAcceleration`, or write members (MaxWalkSpeed, MaxAcceleration, GroundFriction, BrakingDecelerationWalking) | Sprint, ADS, per-weapon move tuning |
| Transient impulse | `LaunchCharacter`, root motion sources | Dash bursts, knockback |
| Custom physics | MOVE_Custom + `PhysCustom` + full `FSavedMove_Character` subclass loop | Slide, climb, wall-run, jetpack |

Parameter writes need no saved-move work when both sides apply the same value symmetrically — the server and client then agree on the cap every frame without any per-move data. Reserve tier 3 for physics the walking/falling branches genuinely cannot express; its closed loop has ~6 places to break, and each break surfaces as "mode enters but behavior is wrong".

## 4. Key Variables

- `Acceleration` — this frame's input intent, recomputed per frame; `Velocity` — the persistent state.
- `UpdatedComponent` — the capsule that actually collides; the mesh is what gets smoothed, never the capsule.
- `CurrentFloor` — walking validity. Stair-snag, slope-speed, and perch bugs all live here (WalkableFloorAngle, MaxStepHeight, PerchRadiusThreshold).
- `MovementMode` / `CustomMovementMode` — the physics branch selector.

## Verification Checklist

- Breakpoint in `PhysWalking`: watch `Acceleration`, `Velocity`, and the value `GetMaxSpeed` returns on the simulating side.
- `p.NetShowCorrections 1` to visualize correction events; when rubberbanding, parity-check your changed variable on BOTH sides before touching anything else.
- "Parameter set but nothing changed" almost always means the write landed on the wrong side (server-only or client-only) — not a physics bug.

## Architecture

CMC is a `UPawnMovementComponent` owned by `ACharacter`. It owns input-to-physics (`CalcVelocity`), collision movement (`MoveUpdatedComponent` family), floor knowledge (`CurrentFloor`), the mode dispatch (`StartNewPhysics`), and the whole prediction/correction loop (`SavedMoves`, `ReplicateMoveToServer`, `ClientAdjustPosition`). It is not a "velocity component" — treating it as one and poking `SetActorLocation` or raw velocity from game code bypasses collision, floor state, and prediction at the same time.

## Data Flow

```
Input → Acceleration
  │
  ▼
PerformMovement ─ StartNewPhysics ─ PhysWalking/PhysFalling/PhysCustom
  │                                   │
  │                                   ▼
  │                             CalcVelocity (friction/braking/GetMaxSpeed)
  │                                   │
  │                                   ▼
  │                             MoveAlongFloor → collision → StepUp/Slide
  │                                   │
  ▼                                   ▼
SavedMoves ─ ReplicateMoveToServer ─ Server re-simulates
  │                                   │
  └──── mismatch ← ClientAdjustPosition ┘
        → rewind + PrepMoveFor replay → SmoothCorrection
```

## Key Claims

- [extracted] `UCharacterMovementComponent::PerformMovement` is defined at `Source/Runtime/Engine/Private/Components/CharacterMovementComponent.cpp:2679` and is the single physics entry shared by local prediction and server re-simulation.
- [extracted] `UCharacterMovementComponent::CalcVelocity` is defined at `Source/Runtime/Engine/Private/Components/CharacterMovementComponent.cpp:3759` and computes Velocity from input acceleration, friction, and braking, clamped by GetMaxSpeed.
- [extracted] `UCharacterMovementComponent::GetMaxSpeed` is defined at `Source/Runtime/Engine/Private/Components/CharacterMovementComponent.cpp:3483` and is the supported per-frame override point for variable speed.
- [extracted] `FSavedMove_Character` is defined at `Source/Runtime/Engine/Classes/GameFramework/CharacterMovementComponent.h:2895` and is the per-move record that carries custom state through prediction and to the server.
- [inferred] A parameter written symmetrically on client and server (e.g. by an ability active on both) needs no saved-move data, because CalcVelocity reads it identically on both sides every frame.
- [inferred] Corrections scale with the divergence window: state applied at different logical times on the two sides produces corrections proportional to RTT, absorbed by network smoothing at moderate speeds.

## Edge Cases

- `SetActorLocation` / raw velocity writes from game code bypass collision, floor state, and the prediction loop — the usual source of "works offline, rubberbands online".
- MaxWalkSpeed is only the ceiling: start/stop/turn weight comes from MaxAcceleration, BrakingDecelerationWalking, and GroundFriction — tuning only the cap leaves the feel unchanged.
- Simulated proxies never run Phys* functions; their smoothness is replicated velocity plus NetworkSmoothingMode, so server-only parameters still affect what viewers see only through the replicated result.
- Jump-related mode quirks (custom modes report not-on-ground) break CanAttemptJump unless explicitly handled.

## Boundaries

- The CMC serves `ACharacter` only; plain pawns use `UPawnMovementComponent` floating movement with no prediction.
- Vehicles and full physics bodies belong to Chaos vehicles/physics, not to CMC custom modes.
- Root motion from animation is consumed inside PerformMovement; the CMC does not author root motion.

## Evidence

- `UCharacterMovementComponent` defined at `Source/Runtime/Engine/Classes/GameFramework/CharacterMovementComponent.h:135`
- `FSavedMove_Character` defined at `Source/Runtime/Engine/Classes/GameFramework/CharacterMovementComponent.h:2895`
- `UCharacterMovementComponent::TickComponent` defined at `Source/Runtime/Engine/Private/Components/CharacterMovementComponent.cpp:1598`
- `UCharacterMovementComponent::PerformMovement` defined at `Source/Runtime/Engine/Private/Components/CharacterMovementComponent.cpp:2679`
- `UCharacterMovementComponent::StartNewPhysics` defined at `Source/Runtime/Engine/Private/Components/CharacterMovementComponent.cpp:3427`
- `UCharacterMovementComponent::CalcVelocity` defined at `Source/Runtime/Engine/Private/Components/CharacterMovementComponent.cpp:3759`
- `UCharacterMovementComponent::PhysWalking` defined at `Source/Runtime/Engine/Private/Components/CharacterMovementComponent.cpp:5527`
- `UCharacterMovementComponent::ReplicateMoveToServer` defined at `Source/Runtime/Engine/Private/Components/CharacterMovementComponent.cpp:8710`
- `UCharacterMovementComponent::ClientAdjustPosition_Implementation` defined at `Source/Runtime/Engine/Private/Components/CharacterMovementComponent.cpp:10979`
