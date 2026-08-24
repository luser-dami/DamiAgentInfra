---
feature: weapon-spread-heat
module: LyraGame/Weapons
tags: [spread, heat, accuracy, curves, ranged]
source: manual
---

# Weapon Spread & Heat

The ranged-weapon accuracy model: every shot adds **heat**, heat maps through
curves to a **spread angle**, and continuous fire is punished while pauses let
the weapon cool down. It lives entirely inside `ULyraRangedWeaponInstance` and
is consumed by the ranged firing ability when computing trace directions. It
is documented separately because it is the single most-tuned piece of weapon
behavior and changes independently of the rest of the module.

## Context

- **Owning module:** `Source/LyraGame/Weapons/`
- **Trigger / Inputs:** each fired shot (heat in), time passing (cooldown), player movement/aim state (multipliers)
- **Consumers:** `ULyraGameplayAbility_RangedWeapon` reads the final spread angle when spawning traces

## Data Flow

The per-shot and per-frame update cycle:

```
Shot fired
   │
   ▼
AddSpread: HeatPerShot = HeatToHeatPerShotCurve.Eval(CurrentHeat)
           CurrentHeat = ClampHeat(CurrentHeat + HeatPerShot)
           CurrentSpreadAngle = HeatToSpreadCurve.Eval(CurrentHeat)
   │
   ▼ (every tick)
UpdateSpread: after SpreadRecoveryCooldownDelay,
           CurrentHeat cools at HeatToCoolDownPerSecondCurve.Eval(CurrentHeat) per second
   │
   ▼ (every tick)
UpdateMultipliers: aiming / standing-still / moving multipliers
           combine into CurrentSpreadAngleMultiplier
   │
   ▼
Ability reads GetCalculatedSpreadAngle() * GetCalculatedSpreadAngleMultiplier()
```

Three designer-edited curves drive everything: `HeatToSpreadCurve` (heat →
angle), `HeatToHeatPerShotCurve` (heat → heat added per shot), and
`HeatToCoolDownPerSecondCurve` (heat → cooldown rate). Because all three are
keyed on current heat, designers can punish overheating progressively.

## Edge Cases

- `bHasFirstShotAccuracy` zeroes the *multiplier* for a perfectly still,
  perfectly aimed first shot — the base angle from heat still applies.
- `SpreadRecoveryCooldownDelay` gates cooling: firing again inside the delay
  keeps the weapon hot, so tap-firing slowly is the accuracy-optimal pattern.
- `SpreadExponent` biases random spread toward the center line; values above
  1.0 cluster shots tighter than a uniform distribution.
- On equip, debug builds seed `CurrentHeat` to the mid-range so the debug
  visualization is immediately meaningful; shipping builds start cold.

## Boundaries

- The spread-heat model does **not** decide where a trace hits — it only produces the
  cone angle; trace execution belongs to the firing ability.
- The spread-heat model does **not** cover recoil, camera kick, or animation-driven accuracy.
- The spread-heat model does **not** persist heat across unequip/equip cycles; state resets.

## Key Claims

- [extracted] `ULyraRangedWeaponInstance` is defined at `Source/LyraGame/Weapons/LyraRangedWeaponInstance.h:20` and owns all heat/spread state and curves.
- [extracted] `ComputeSpreadRange` is defined at `Source/LyraGame/Weapons/LyraRangedWeaponInstance.h:233` and derives the min/max spread angles from the heat curve's key range.
- [extracted] `ClampHeat` is defined at `Source/LyraGame/Weapons/LyraRangedWeaponInstance.h:236` and bounds heat to the designer-set heat range.
- [inferred] The three heat-keyed curves are a deliberate design lever: one heat value drives heat-in, spread-out, and cooldown, so balance changes touch data, not code.
- [inferred] Heat state is intentionally transient — it is recomputed per equip and never replicated as authoritative player state.

## Evidence

- `ULyraRangedWeaponInstance` defined at `Source/LyraGame/Weapons/LyraRangedWeaponInstance.h:20`
- `ComputeSpreadRange` defined at `Source/LyraGame/Weapons/LyraRangedWeaponInstance.h:233`
- `ClampHeat` defined at `Source/LyraGame/Weapons/LyraRangedWeaponInstance.h:236`
- `ULyraGameplayAbility_RangedWeapon` defined at `Source/LyraGame/Weapons/LyraGameplayAbility_RangedWeapon.h:47`
