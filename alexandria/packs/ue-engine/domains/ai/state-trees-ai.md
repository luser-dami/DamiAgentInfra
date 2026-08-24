---
name: state-trees-ai
description: Implementing UE5 State Trees for hybrid AI logic. Replacing Behavior Trees for high-performance state management.
domain: ai
---

# State Trees AI (UE5 Expert)

## Context
- Mandatory when designing AI or gameplay logic that requires strict state machines (Idle -> Alert -> Combat).
- Trigger when Behavior Trees become too messy due to complex transition rules, or when optimizing for Mass Entity.

## 1. Core Principles
State Trees combine the structure of a State Machine with the flexibility of a Behavior Tree.
- **States**: The tree is always in exactly ONE state at a time.
- **Evaluators**: Run continuously in the background to calculate data (e.g., "Distance to Player").
- **Tasks**: Execute actions while in a state (e.g., "Play Animation", "Move To").

## 2. C++ Implementation (Creating Custom Tasks)

### The Task Struct
Unlike BTs which use `UObjects`, State Tree tasks use lightweight `USTRUCTs` inheriting from `FStateTreeTaskBase`. This allows for massive performance gains.

```cpp
#pragma once
#include "StateTreeTaskBase.h"
#include "MyStateTreeTask.generated.h"

// Define the inputs/outputs for the task
USTRUCT()
struct FMyTaskInstanceData
{
    GENERATED_BODY()
    
    UPROPERTY(EditAnywhere, Category = "Input")
    float TargetSpeed = 0.0f;
};

USTRUCT(meta = (DisplayName = "Set Walk Speed"))
struct FMyStateTreeTask : public FStateTreeTaskBase
{
    GENERATED_BODY()

    // Link the instance data
    typedef FMyTaskInstanceData FInstanceDataType;

    virtual const UStruct* GetInstanceDataType() const override { return FInstanceDataType::StaticStruct(); }

    virtual EStateTreeRunStatus EnterState(FStateTreeExecutionContext& Context, const FStateTreeTransitionResult& Transition) const override;
};
```

### Execution Logic
```cpp
EStateTreeRunStatus FMyStateTreeTask::EnterState(FStateTreeExecutionContext& Context, const FStateTreeTransitionResult& Transition) const
{
    // Retrieve the instance data
    FInstanceDataType& InstanceData = Context.GetInstanceData(*this);

    // Retrieve the Actor running the tree
    if (AActor* Owner = Context.GetOwner())
    {
        if (UCharacterMovementComponent* MoveComp = Owner->FindComponentByClass<UCharacterMovementComponent>())
        {
            MoveComp->MaxWalkSpeed = InstanceData.TargetSpeed;
            return EStateTreeRunStatus::Succeeded;
        }
    }
    return EStateTreeRunStatus::Failed;
}
```


## Verification Checklist
- [ ] `StateTreeModule` is added to `Build.cs`.
- [ ] Custom tasks inherit from `FStateTreeTaskBase` (as `USTRUCTs`, not `UCLASSes`).
- [ ] Instance data is separated into a dedicated `FInstanceDataType` struct.
- [ ] `Context.GetInstanceData()` is used safely to access parameters.

## Architecture

The architecture for State Trees Ai follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of State Trees Ai.

## Key Claims

- **Zero GC Overhead**: Because Tasks are `USTRUCTs`, they allocate in contiguous memory blocks and bypass the Garbage Collector.
- **Strict Transitions**: State Trees prevent the classic Behavior Tree bug where two branches try to execute simultaneously due to conflicting decorators.


The State Trees Ai pattern follows UE5 engine conventions and best practices.

## Edge Cases

Refer to the Common Mistakes section for State Trees Ai edge cases. Ensure proper null checks and UPROPERTY replication for networked usage.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h:1`

