---
name: blackboard-data-access
description: Safe interaction with UBlackboardComponent from C++. Manipulating Keys using FBlackboardKeySelector.
domain: ai
---

# Blackboard Data Access (UE5 Expert)

## Context
- Mandatory when writing C++ Behavior Tree Tasks, Services, or Decorators that need to read/write memory.
- Trigger when the AI Controller needs to pass a target (Actor or Location) to the Behavior Tree.

## 1. Core Principles
The `UBlackboardComponent` is a dictionary of typed variables (Keys). Accessing them requires the exact `FName` of the key, but the safest method is using `FBlackboardKeySelector`.

## 2. Using Key Selectors (In BT Nodes)
This exposes a dropdown in the BT Editor for the designer to pick the key, avoiding hardcoded strings in C++.

### Header
```cpp
UPROPERTY(EditAnywhere, Category = "Blackboard")
FBlackboardKeySelector TargetActorKey;

UPROPERTY(EditAnywhere, Category = "Blackboard")
FBlackboardKeySelector DestinationVectorKey;
```

### Source
Use the `OwnerComp.GetBlackboardComponent()` to read/write.
```cpp
UBlackboardComponent* BB = OwnerComp.GetBlackboardComponent();

// Writing an Object
BB->SetValueAsObject(TargetActorKey.SelectedKeyName, FoundEnemy);

// Reading a Vector
FVector Dest = BB->GetValueAsVector(DestinationVectorKey.SelectedKeyName);

// Clearing a Key
BB->ClearValue(TargetActorKey.SelectedKeyName);
```

## 3. Direct Access (From AI Controller)
If you are pushing data from the Controller into the Blackboard (not inside a BT Node), you must use hardcoded `FNames`.

```cpp
void AMyAIController::OnSensePlayer(AActor* Player)
{
    if (UBlackboardComponent* BB = GetBlackboardComponent())
    {
        // "TargetEnemy" must perfectly match the key name in the Blackboard Asset
        BB->SetValueAsObject(FName("TargetEnemy"), Player);
    }
}
```


## Edge Cases
**BAD (Hardcoding in BT Nodes)**:
```cpp
BB->SetValueAsObject(FName("CurrentTarget"), Enemy); // BAD: Breaks if designer renames the key.
```

**GOOD (Key Selector)**:
```cpp
BB->SetValueAsObject(TargetKey.SelectedKeyName, Enemy); // SAFE: Configured in Editor.
```

## Verification Checklist
- [ ] `FBlackboardKeySelector` is used for all BT Node memory access.
- [ ] Hardcoded `FNames` are used ONLY when injecting data from outside the Behavior Tree (e.g., from the AIController).
- [ ] Getters/Setters strictly match the Blackboard Key type (e.g., `GetValueAsEnum`, `SetValueAsBool`).

## Architecture

The architecture for Blackboard Data Access follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Blackboard Data Access.

## Key Claims

- **Type Mismatch**: Calling `SetValueAsObject` on a key that was defined as a `Vector` in the Blackboard Asset will fail silently. Always double-check types.
- **String Typo Safety**: Hardcoded `FName`s are brittle. If a designer renames the key in the Blackboard Asset, the C++ code breaks silently. `FBlackboardKeySelector` protects against this by linking by reference in the BT Editor.


The Blackboard Data Access pattern follows UE5 engine conventions and best practices.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h`

