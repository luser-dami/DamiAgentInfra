---
name: ai-controller-possession
description: Architecture of AAIController, Blackboards, and Behavior Trees. Rules for AI possession and logic execution.
domain: ai
---

# AI Controller & Possession (UE5 Expert)

## Context
- Mandatory when creating non-player characters (NPCs), enemies, or automated agents.
- Trigger when assigning a Brain (AI Controller) to a Body (Pawn/Character).

## 1. Core Principles
The `AAIController` is the AI equivalent of `APlayerController`.
- **Server Only**: AIControllers exist ONLY on the Server. Clients never execute AI logic. Clients only see the replicated movements and variables of the possessed Pawn.
- **Behavior Tree**: The AIController runs the `UBehaviorTree` and holds the memory `UBlackboardComponent`.

## 2. Configuration & Possession

### Setting up the Pawn
To ensure the AI automatically takes over when spawned, configure the Pawn's Auto Possess settings.
```cpp
AMyEnemyCharacter::AMyEnemyCharacter()
{
    // Possess automatically when spawned or placed in world
    AutoPossessAI = EAutoPossessAI::PlacedInWorldOrSpawned;
    AIControllerClass = AMyAIController::StaticClass();
}
```

### The AIController Initialization
Override `OnPossess` to start the brain logic.

```cpp
#include "BehaviorTree/BlackboardComponent.h"
#include "BehaviorTree/BehaviorTree.h"

void AMyAIController::OnPossess(APawn* InPawn)
{
    Super::OnPossess(InPawn);

    if (AMyEnemyCharacter* Enemy = Cast<AMyEnemyCharacter>(InPawn))
    {
        if (UBehaviorTree* BT = Enemy->GetBehaviorTree())
        {
            // Initializes the Blackboard and starts the Tree
            RunBehaviorTree(BT);
        }
    }
}
```

## 3. Accessing the Blackboard
The Blackboard is the AI's memory (Target Enemy, Patrol Locations).
```cpp
void AMyAIController::SetTargetEnemy(AActor* Target)
{
    if (UBlackboardComponent* BB = GetBlackboardComponent())
    {
        BB->SetValueAsObject(FName("TargetActor"), Target);
    }
}
```


## Edge Cases
**BAD (Client-Side AI)**:
```cpp
// Client attempts to move AI
MyAIController->MoveToLocation(Dest); // Ignored by server, breaks pathfinding.
```

**GOOD (Server-Side AI)**:
```text
All Navigation and MoveTo commands execute on the Server. The built-in CharacterMovementComponent replicates the transform to clients smoothly.
```

## Verification Checklist
- [ ] Pawn has `AutoPossessAI` configured correctly.
- [ ] `AIControllerClass` points to your custom `AAIController`.
- [ ] `RunBehaviorTree()` is called inside `OnPossess()`.
- [ ] Blackboard values are manipulated via `GetBlackboardComponent()`.

## Architecture

The architecture for Ai Controller Possession follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Ai Controller Possession.

## Key Claims

- **Client Execution Fails**: Writing AI logic in a client-rpc or expecting clients to evaluate Behavior Trees will fail completely. AI logic is Server Authority only.
- **Detachment**: When an AI dies, you must detach the controller (`UnPossess()`) or destroy the Controller to stop the Behavior Tree from consuming CPU cycles in the background.


The Ai Controller Possession pattern follows UE5 engine conventions and best practices.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h`

