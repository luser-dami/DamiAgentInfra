---
name: enhanced-input-triggers
description: Enhanced Input trigger internals — ETriggerEvent semantics, the trigger state machine, and the trigger-type matrix that decides when your bindings actually fire.
module: enhanced-input-triggers
---

# Enhanced Input: Triggers & ETriggerEvent (UE5 Expert)

## Context
- Mandatory before binding anything to `ETriggerEvent::Completed`/`Started`, or mixing tap and hold behaviors on one key.
- Classic bug this kills: a hold-to-activate feature that ends instantly because the input action uses a pulse trigger whose Completed fires in the same frame as the press.

## 1. Pipeline

```
Hardware input → mapping context (IMC: key → InputAction + triggers)
        │
        ▼
UEnhancedInputComponent bindings (function per ETriggerEvent)
        │
        ▼
UEnhancedPlayerInput evaluates each trigger's state machine per frame
        │
        ▼
ETriggerEvent fired → your bound functions
```

An InputAction with **no triggers** behaves as a plain state: actuated = Triggered every frame, release = Completed. That is the correct form for hold-driven bindings.

## 2. ETriggerEvent Semantics

| Event | Fires |
|-------|-------|
| Started | Once, when the input crosses the actuation threshold |
| Ongoing | Every frame while actuated but trigger conditions not yet met |
| Triggered | Every frame while the trigger's conditions are satisfied |
| Completed | Once, when a satisfied trigger finishes (release for state triggers, **immediately** for pulse triggers) |
| Canceled | Once, when a trigger fails mid-evaluation (e.g. released before HoldTime) |

The trap: **Completed is about the trigger's lifecycle, not about the physical key.** Bindings that mean "when the player lets go" only work if the trigger type's Completed coincides with release.

## 3. Trigger-Type Matrix

| Trigger | Triggered | Completed | Use for |
|---------|-----------|-----------|---------|
| (none) | every frame while down | on release | hold-to-run, auto fire |
| Down | every frame while down | on release | same, explicit |
| Pressed | once on press | **same frame** | one-shot fire, dash |
| Released | once on release | same frame | fire-on-release |
| Tap | once if released within threshold | same frame | tap vs hold disambiguation |
| Hold | once after HoldTime | on release | charge abilities |
| HoldAndRelease | after HoldTime | on release (with held duration) | charged shots |
| Pulse | every interval while down | per pulse | repeat-while-held |

## 4. Composition Rules and Their Failure Mode

Multiple triggers on one InputAction are evaluated independently and **all of them drive the same events**. A `Pressed` + `Hold` combo for "tap = dash, hold = sprint" looks right and is wrong: the Pressed trigger's Completed fires on the press frame and any Completed-bound "stop" logic runs instantly. The correct shape is two InputActions on the same key in the IMC — each with its own trigger type — so each event stream stays clean.

## Verification Checklist

- Hold feature ends instantly: dump the InputAction's triggers — a pulse trigger is almost certainly present.
- Bindings firing every frame unexpectedly: you bound Triggered on a state trigger — that's what it does; gate your logic or use Started.
- Chorded combos dropping: chord triggers gate on another action's state; check actuation order and thresholds.

## Architecture

Triggers are a per-action state machine (None → Started → Ongoing → Triggered → Completed/Canceled) evaluated by `UEnhancedPlayerInput` before any delegate fires. Everything downstream — Lyra's ability-input bridge included — sees only the resulting ETriggerEvent stream. Designing input behavior means choosing the trigger state machine first and the bindings second, never the reverse.

## Data Flow

```
Key down
  │
  ▼
Trigger state machine evaluates
  ├─ state trigger (Down/none): Started → Triggered (per frame) → release → Completed
  └─ pulse trigger (Pressed):   Started → Triggered → Completed (all one frame)
  │
  ▼
ETriggerEvent stream → bound handlers
  ├─ Triggered-bound: "pressed" logic
  └─ Completed-bound: "released" logic — only valid if the trigger type completes on release
```

## Key Claims

- [extracted] `ETriggerEvent` is defined at `Plugins/EnhancedInput/Source/EnhancedInput/Public/InputTriggers.h:34` with Started/Ongoing/Triggered/Completed/Canceled semantics.
- [extracted] `UInputTriggerPressed` is defined at `Plugins/EnhancedInput/Source/EnhancedInput/Public/InputTriggers.h:253` and is a one-shot pulse that completes in the frame it triggers.
- [extracted] `UInputTriggerDown` is defined at `Plugins/EnhancedInput/Source/EnhancedInput/Public/InputTriggers.h:236` and completes on physical release.
- [extracted] `UInputTriggerHold` is defined at `Plugins/EnhancedInput/Source/EnhancedInput/Public/InputTriggers.h:292` and triggers only after its hold time elapses.
- [inferred] Any design needing both tap and hold on one physical key requires two InputActions on that key, because every trigger on a single action shares the same ETriggerEvent stream.

## Edge Cases

- An InputAction with zero triggers is valid and is the standard hold form — absence of triggers is not an error.
- `ETriggerEvent::Triggered` on a state trigger fires every frame; per-frame handlers must be idempotent and cheap.
- Releasing before a Hold trigger's HoldTime produces Canceled, not Completed — bindings attached only to Completed never hear about it.

## Boundaries

- Trigger semantics are the scope here; mapping-context priority, modifiers (negate/swizzle/deadzone), and input processors are separate layers.
- Platform-specific key translation belongs to the IMC assets, not to trigger choice.

## Evidence

- `ETriggerEvent` defined at `Plugins/EnhancedInput/Source/EnhancedInput/Public/InputTriggers.h:34`
- `UInputTrigger` defined at `Plugins/EnhancedInput/Source/EnhancedInput/Public/InputTriggers.h:113`
- `UInputTriggerDown` defined at `Plugins/EnhancedInput/Source/EnhancedInput/Public/InputTriggers.h:236`
- `UInputTriggerPressed` defined at `Plugins/EnhancedInput/Source/EnhancedInput/Public/InputTriggers.h:253`
- `UInputTriggerHold` defined at `Plugins/EnhancedInput/Source/EnhancedInput/Public/InputTriggers.h:292`
- `UInputTriggerTap` defined at `Plugins/EnhancedInput/Source/EnhancedInput/Public/InputTriggers.h:338`
- `UEnhancedInputComponent` defined at `Plugins/EnhancedInput/Source/EnhancedInput/Public/EnhancedInputComponent.h:354`
- `UEnhancedPlayerInput` defined at `Plugins/EnhancedInput/Source/EnhancedInput/Public/EnhancedPlayerInput.h:94`
