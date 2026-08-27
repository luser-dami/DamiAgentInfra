---
name: gas-ugameplayeffect
description: Configuration of UGameplayEffect for applying damage, buffs, and modifying attributes. Outlines Duration Policies and Tags.
domain: gas
---

# GAS: UGameplayEffect (UE5 Expert)

## Context
- Mandatory for modifying ANY value in a `UAttributeSet` (Damage, Healing, Speed Buffs).
- Trigger when granting Gameplay Tags temporarily to an Actor (e.g., granting `State.Stunned` for 5 seconds).

## 1. Core Principles
`UGameplayEffect` (GE) is a data-only class (usually constructed entirely in Blueprint, rarely C++). It is the payload delivered by abilities or environmental hazards.
- **Data Driven**: You do not write logic in a GE. You configure arrays of Modifiers and Tags.

## 2. Duration Policies
- **Instant**: Applies permanently to the Base Value of an attribute. (e.g., Permanent Damage or Permanent Healing).
- **Has Duration**: Applies temporarily for X seconds to the Current Value. Reverts automatically. (e.g., +50 Armor for 10s).
- **Infinite**: Applies until explicitly removed. (e.g., A cursed ring that removes -10 Strength while equipped).

## 3. Modifiers vs Executions
- **Modifiers**: Simple mathematical operations (Add, Multiply, Divide) on an Attribute. (e.g., `Health Add -50`).
- **Executions (Calculation Class)**: Uses a C++ `UGameplayEffectExecutionCalculation` class. Required for complex formulas involving the Instigator's stats and the Target's stats (e.g., `Damage = BaseDamage * (1 - TargetArmor/100)`).

## 4. Tag Configuration (The Heart of GE)
Gameplay Effects manage state via Tags.

- **GrantedTags**: Tags given to the Target while the GE is active. (e.g., An "On Fire" GE grants `State.Burning`).
- **AssetTags**: Tags describing the GE itself for identification. (e.g., `Effect.Damage.Fire`).
- **ApplicationRequiredTags**: The Target MUST have these tags for the GE to apply. (e.g., A "Cure Poison" potion GE requires `State.Poisoned`).
- **RemoveGameplayEffectsWithTags**: When applied, this GE instantly removes other GEs matching these tags. (e.g., "Cure Poison" removes all GEs with `Effect.Debuff.Poison`).

## 5. Application via C++
Always apply GEs using the Ability System Component (ASC) to ensure replication and prediction work.

```cpp
// Generating the Spec
FGameplayEffectContextHandle ContextHandle = ASC->MakeEffectContext();
ContextHandle.AddInstigator(Instigator, Causer);

FGameplayEffectSpecHandle SpecHandle = ASC->MakeOutgoingSpec(DamageEffectClass, 1.0f, ContextHandle);

if (SpecHandle.IsValid())
{
    // Dynamic Set-By-Caller (Pass variables from code into the GE)
    SpecHandle.Data.Get()->SetSetByCallerMagnitude(TAG_Data_DamageAmount, 50.0f);
    
    // Application
    ASC->ApplyGameplayEffectSpecToTarget(*SpecHandle.Data.Get(), TargetASC);
}
```


## Edge Cases
**BAD (Direct Attribute Modification)**:
```cpp
TargetASC->GetAttributeSet()->SetHealth(50); // FATAL: Bypasses prediction, immunities, and tag blocking.
```

**GOOD (Effect Application)**:
```cpp
ASC->ApplyGameplayEffectToSelf(DamageEffectCDO, ...); // SAFE: respects GAS rules.
```

## Verification Checklist
- [ ] GEs modifying persistent health/mana use `Instant` duration.
- [ ] Buffs/Debuffs use `Has Duration` or `Infinite`.
- [ ] `ApplyGameplayEffectSpecToTarget` is used to trigger the effect.
- [ ] Dynamic values are passed using `SetByCallerMagnitude`.

## Architecture

The architecture for Gas Ugameplayeffect follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Gas Ugameplayeffect.

## Key Claims

- **Instant vs Duration**: Modifying `Health` with a "Has Duration" effect means when the duration ends, the player heals back the damage automatically. Instant must be used for permanent loss.
- **Prediction**: GEs are automatically predicted locally if configured correctly, preventing laggy hit feedback.


The Gas Ugameplayeffect pattern follows UE5 engine conventions and best practices.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h`

