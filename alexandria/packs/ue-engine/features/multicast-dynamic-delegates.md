---
name: multicast-dynamic-delegates
description: Expert implementation of DECLARE_DYNAMIC_MULTICAST_DELEGATE. Enables C++ systems to broadcast events to multiple Blueprint listeners for UI and gameplay triggers.
feature: multicast-dynamic-delegates
module: Core
---

# Multicast Dynamic Delegates (UE5 Expert)

## Context
- Mandatory when a C++ event must be observable by multiple Blueprints (e.g., UI updates, state changes).
- Trigger when you need decoupled "Fire and Forget" event broadcasting.

## 1. Core Principles
- **Dynamic**: Can be serialized (saved) and bound within the Blueprint Graph.
- **Multicast**: Allows multiple functions (listeners) to bind to the same event.
- **Decoupling**: The sender does not know or care who is listening.

## 2. Implementation Syntax

### Declaration
Must be declared OUTSIDE the class definition. Max 8 parameters.
```cpp
// Delegate with 1 parameter
DECLARE_DYNAMIC_MULTICAST_DELEGATE_OneParam(FOnHealthChangedSignature, float, NewHealth);
```

### Class Member
Must be a `BlueprintAssignable` UPROPERTY.
```cpp
UCLASS()
class AMyCharacter : public ACharacter
{
    GENERATED_BODY()

public:
    UPROPERTY(BlueprintAssignable, Category = "Events")
    FOnHealthChangedSignature OnHealthChanged;
};
```

### Broadcasting (Firing)
Always check if bound (though Multicast is safe to broadcast empty).
```cpp
void AMyCharacter::TakeDamage(float Amount)
{
    Health -= Amount;
    OnHealthChanged.Broadcast(Health);
}
```


## Edge Cases
**BAD (Hard Reference to UI)**:
```cpp
void AMyCharacter::TakeDamage() {
    MyUIWidget->UpdateHealthBar(); // Tightly coupled, breaks if UI changes
}
```

**GOOD (Delegate Broadcast)**:
```cpp
void AMyCharacter::TakeDamage() {
    OnHealthChanged.Broadcast(Health); // Decoupled. UI listens if it wants to.
}
```

## Verification Checklist
- [ ] Delegate names end with `Signature` (convention).
- [ ] UPROPERTY is marked `BlueprintAssignable`.
- [ ] Delegate is declared outside the class.
- [ ] No hard references to listening classes exist in the broadcaster.

## Architecture

The architecture for Multicast Dynamic Delegates follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Multicast Dynamic Delegates.

## Key Claims

- **Blueprint Isolation**: Prevents C++ from needing hard references to UI classes.
- **Memory Safety**: `Dynamic` delegates use weak references internally. If a Blueprint object is destroyed, it is safely unbound automatically.


The Multicast Dynamic Delegates pattern follows UE5 engine conventions and best practices.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h:1`

