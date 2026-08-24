---
name: gas-target-data
description: Handling FGameplayAbilityTargetData. Synchronizing targeting info (HitResults, Locations) between Client and Server in GAS.
domain: gas
---

# GAS: Target Data Handling (UE5 Expert)

## Context
- Mandatory when a `UGameplayAbility` needs to know *what* or *where* to hit (e.g., an AoE spell location, a Raycast hit).
- Trigger when you must send target information from the local client to the server for verification.

## 1. Core Principles
`FGameplayAbilityTargetData` is a polymorphic struct that holds targeting info (Actors, HitResults, Vectors).
Because clients often perform targeting (like clicking a location on their screen), this data must be explicitly synchronized to the server.

## 2. Generating Target Data
Usually generated via `WaitTargetData` tasks, but can be built manually in C++.

```cpp
// Manual Generation
FGameplayAbilityTargetDataHandle TargetDataHandle;

// Create Hit Result Data
FGameplayAbilityTargetData_SingleTargetHit* HitData = new FGameplayAbilityTargetData_SingleTargetHit();
HitData->HitResult = MyHitResult;

TargetDataHandle.Add(HitData);
```

## 3. Client-to-Server Synchronization
If the local client generates the target data, you MUST send it to the server.

```cpp
void UMyGameplayAbility::SendTargetDataToServer(const FGameplayAbilityTargetDataHandle& TargetData)
{
    if (IsPredictingClient())
    {
        // Creates a scoped prediction window
        FScopedPredictionWindow ScopedPrediction(GetAbilitySystemComponent(), true);

        // Tell the server we have data
        GetAbilitySystemComponent()->CallServerSetReplicatedTargetData(
            CurrentSpecHandle, 
            CurrentActivationInfo.GetActivationPredictionKey(), 
            TargetData, 
            FGameplayTag(), 
            GetAbilitySystemComponent()->ScopedPredictionKey);
    }
}
```

## 4. Consuming Target Data on the Server
The Server must wait to receive the data before applying the effect.

```cpp
void UMyGameplayAbility::OnTargetDataReceived(const FGameplayAbilityTargetDataHandle& Data, FGameplayTag ActivationTag)
{
    GetAbilitySystemComponent()->ConsumeClientReplicatedTargetData(CurrentSpecHandle, CurrentActivationInfo.GetActivationPredictionKey());

    // Apply the GameplayEffect using the verified TargetData
    ApplyGameplayEffectToTarget(CurrentSpecHandle, CurrentActorInfo, CurrentActivationInfo, Data, DamageEffectClass, 1.0f);
    
    EndAbility(CurrentSpecHandle, CurrentActorInfo, CurrentActivationInfo, true, false);
}
```


## Verification Checklist
- [ ] Local client uses `CallServerSetReplicatedTargetData`.
- [ ] Server uses `ConsumeClientReplicatedTargetData`.
- [ ] Server validates the physical possibility of the TargetData before applying effects.

## Architecture

The architecture for Gas Target Data follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Gas Target Data.

## Key Claims

- **Anti-Cheat**: If you don't validate the TargetData on the server (e.g., checking if the HitLocation is actually within line of sight), a hacked client can send TargetData hitting an enemy behind a wall across the map.
- **Memory Leaks**: `FGameplayAbilityTargetDataHandle` uses Shared Pointers internally to manage the raw `new` allocations. Never manage the raw pointers manually after adding them to the Handle.


The Gas Target Data pattern follows UE5 engine conventions and best practices.

## Edge Cases

Refer to the Common Mistakes section for Gas Target Data edge cases. Ensure proper null checks and UPROPERTY replication for networked usage.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h:1`

