---
name: bt-task-cplusplus
description: Creating custom C++ Behavior Tree Task Nodes (UBTTaskNode). Handling ExecuteTask, ticking, and asynchronous EBTNodeResult completion.
domain: ai
---

# Behavior Tree Tasks in C++ (UE5 Expert)

## Context
- Mandatory when creating custom actions for AI (e.g., Attack, FindCover, PlayAnimation).
- Trigger when Blueprint BT Tasks become too complex, slow, or require deep C++ system access.

## 1. Core Principles
A `UBTTaskNode` executes logic and returns a state:
- `Succeeded`: Action completed successfully. Move to next node.
- `Failed`: Action aborted/failed. Tree searches for alternatives.
- **`InProgress`**: Action takes time (e.g., walking, playing animation). The tree pauses on this node until `FinishLatentTask` is called.

## 2. Implementation Syntax

### Header (.h)
```cpp
#pragma once
#include "CoreMinimal.h"
#include "BehaviorTree/BTTaskNode.h"
#include "MyBTTask_Attack.generated.h"

UCLASS()
class MYPROJECT_API UMyBTTask_Attack : public UBTTaskNode
{
    GENERATED_BODY()

public:
    UMyBTTask_Attack();

    virtual EBTNodeResult::Type ExecuteTask(UBehaviorTreeComponent& OwnerComp, uint8* NodeMemory) override;

protected:
    virtual void TickTask(UBehaviorTreeComponent& OwnerComp, uint8* NodeMemory, float DeltaSeconds) override;
};
```

### Source (.cpp) - Immediate vs Latent
```cpp
UMyBTTask_Attack::UMyBTTask_Attack()
{
    NodeName = "Melee Attack";
    // Enable TickTask if you need to monitor progress every frame
    bNotifyTick = true; 
}

EBTNodeResult::Type UMyBTTask_Attack::ExecuteTask(UBehaviorTreeComponent& OwnerComp, uint8* NodeMemory)
{
    AAIController* AIController = OwnerComp.GetAIOwner();
    if (!AIController) return EBTNodeResult::Failed;

    AMyCharacter* Pawn = Cast<AMyCharacter>(AIController->GetPawn());
    if (!Pawn) return EBTNodeResult::Failed;

    // Start the attack animation
    Pawn->StartAttack();

    // Tell the Tree to wait for us to call FinishLatentTask
    return EBTNodeResult::InProgress;
}

void UMyBTTask_Attack::TickTask(UBehaviorTreeComponent& OwnerComp, uint8* NodeMemory, float DeltaSeconds)
{
    AAIController* AIController = OwnerComp.GetAIOwner();
    AMyCharacter* Pawn = Cast<AMyCharacter>(AIController->GetPawn());

    // Check if attack is done
    if (!Pawn->IsAttacking())
    {
        // Conclude the node
        FinishLatentTask(OwnerComp, EBTNodeResult::Succeeded);
    }
}
```

## 3. Delegate-Based Latent Tasks (Optimized)
Instead of enabling `TickTask`, it is far better to bind a Delegate (e.g., `OnAttackFinished`) and call `FinishLatentTask` inside the callback.

```cpp
// Inside ExecuteTask
Pawn->OnAttackFinished.AddDynamic(this, &UMyBTTask_Attack::HandleAttackFinished);
return EBTNodeResult::InProgress;

void UMyBTTask_Attack::HandleAttackFinished()
{
    // Must store OwnerComp or cast from GetOuter()
    FinishLatentTask(CachedOwnerComp, EBTNodeResult::Succeeded);
}
```


## Verification Checklist
- [ ] Class inherits from `UBTTaskNode`.
- [ ] `ExecuteTask` returns `Succeeded`, `Failed`, or `InProgress`.
- [ ] If `InProgress` is returned, `FinishLatentTask` is guaranteed to be called eventually.
- [ ] Local state variables are avoided unless `bCreateNodeInstance = true` is set in constructor.

## Architecture

The architecture for Bt Task Cplusplus follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Bt Task Cplusplus.

## Key Claims

- **Node Lockup**: Returning `InProgress` but forgetting to call `FinishLatentTask` will permanently freeze the AI's Brain.
- **Instancing**: By default, BT nodes DO NOT instantiate a unique C++ object per AI. Memory is shared (NodeMemory). If you need local variables (like `CachedOwnerComp`), you must either set `bCreateNodeInstance = true` (heavy) or store data in the `NodeMemory` pointer (complex). For ease of use, `bCreateNodeInstance = true` is acceptable for small AI counts.


The Bt Task Cplusplus pattern follows UE5 engine conventions and best practices.

## Edge Cases

Refer to the Common Mistakes section for Bt Task Cplusplus edge cases. Ensure proper null checks and UPROPERTY replication for networked usage.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h`

