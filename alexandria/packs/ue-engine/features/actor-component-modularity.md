---
name: actor-component-modularity
description: Building reusable logic blocks via ActorComponents to avoid deep inheritance trees. Promotes 'Artifact-based' composition.
feature: actor-component-modularity
module: GameFramework
---

# ActorComponent Modularity (UE5 Expert)

## Context
- Mandatory when adding complex logic (Health, Inventory, Movement) to an Actor.
- Use to avoid "God Classes" (e.g., `ABaseCharacter` that handles everything).
- Trigger when behavior needs to be shared across disparate classes (e.g., A player and a destructible barrel both need Health).

## 1. The Composition Principle
"Favor Composition over Inheritance." Instead of deriving `APlayer` from `ADamageableActor`, make `APlayer` inherit from `ACharacter` and attach a `UHealthComponent`.

## 2. Implementation Syntax

### Creating the Component
Inherit from `UActorComponent` (Logic only) or `USceneComponent` (Logic + Transform).
```cpp
UCLASS(ClassGroup=(Custom), meta=(BlueprintSpawnableComponent))
class UHealthComponent : public UActorComponent
{
    GENERATED_BODY()
public:
    UPROPERTY(BlueprintAssignable)
    FOnDeathSignature OnDeath;

    void TakeDamage(float Amount);
};
```

### Attaching in C++ (Constructor)
Components created in the constructor act as the "Default" setup for the Blueprint Archetype.
```cpp
AMyActor::AMyActor()
{
    // Create the component
    HealthComp = CreateDefaultSubobject<UHealthComponent>(TEXT("HealthComponent"));
}
```

### Using the Component
Agents and other systems should check for the component rather than casting the Actor.
```cpp
void ApplyDamage(AActor* Target)
{
    if (UHealthComponent* HC = Target->FindComponentByClass<UHealthComponent>())
    {
        HC->TakeDamage(10.f);
    }
}
```

## 3. Communication Patterns
Components should be **agnostic** of their owner.
- **Component to Owner**: Use Delegates (`OnDeath.Broadcast()`). The Owner binds to the delegate.
- **Owner to Component**: Direct function calls (`HealthComp->Heal()`).
- **Component to Component**: Use `GetOwner()->FindComponentByClass<T>()` to find sibling components.


## Edge Cases
**BAD (Deep Inheritance)**:
```cpp
// Rigid, hard to maintain
class AEntity : public AActor;
class ADamageableEntity : public AEntity;
class AMovableDamageableEntity : public ADamageableEntity;
```

**GOOD (Component Composition)**:
```cpp
// Flexible, plug-and-play
class AEnemy : public AActor {
    UHealthComponent* Health;
    UMovementComponent* Move;
};
```

**BAD (Component hard-coding Owner)**:
```cpp
void UHealthComponent::Die() {
    Cast<AMyPlayer>(GetOwner())->TriggerDeathAnimation(); // BAD: Component now only works on AMyPlayer!
}
```

**GOOD (Delegate broadcasting)**:
```cpp
void UHealthComponent::Die() {
    OnDeath.Broadcast(); // SAFE: Any owner can listen and play its own animation.
}
```

## Verification Checklist
- [ ] Shared logic is extracted into `UActorComponent` derivatives.
- [ ] Components communicate upwards via Delegates, not hard casts.
- [ ] Pointers to default subobjects are marked `VisibleAnywhere` (See UPROPERTY skill).
- [ ] Other classes interact via `FindComponentByClass` or Interfaces.

## Architecture

The architecture for Actor Component Modularity follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Actor Component Modularity.

## Key Claims

- **Artifact Isolation**: Bugs in the Inventory system won't crash the Health system.
- **Iteration Speed**: Designers can mix-and-match components on Blueprints without asking programmers to re-architect the C++ class hierarchy.


The Actor Component Modularity pattern follows UE5 engine conventions and best practices.

## Boundaries
- The Actor Component Modularity pattern covers component-based composition in UE5, not deep inheritance hierarchies.
- Engine systems outside of ActorComponent and SceneComponent are out of scope.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h`

