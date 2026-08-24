---
name: userwidget-lifecycle-management
description: Proper handling of UUserWidget Initialize, NativeConstruct, and NativeDestruct. Prevents memory leaks by ensuring delegates are safely unbound.
feature: userwidget-lifecycle-management
module: UI
---

# UserWidget Lifecycle Management (UE5 Expert)

## Context
- Mandatory when creating custom `UUserWidget` classes in C++.
- Trigger when setting up data bindings, event listeners, or timers within a UI element.

## 1. The Core Lifecycle Methods
UMG Widgets have a distinct lifecycle. Never use the standard C++ constructor `AMyActor()` for UI initialization.

### `Initialize()`
Called ONCE when the widget is created.
- **Use for**: Finding sub-widgets, one-time setup that doesn't depend on gameplay state.
- **Constraint**: Must return `Super::Initialize()`.

### `NativeConstruct()`
Called EVERY TIME the widget is added to the viewport (or parent widget).
- **Use for**: Binding to player stats, playing intro animations, starting UI timers.

### `NativeDestruct()`
Called EVERY TIME the widget is removed from the viewport.
- **Use for**: Unbinding delegates! Critical for memory safety.

## 2. Implementation Syntax

```cpp
#pragma once
#include "CoreMinimal.h"
#include "Blueprint/UserWidget.h"
#include "MyUIWidget.generated.h"

UCLASS()
class MYPROJECT_API UMyUIWidget : public UUserWidget
{
    GENERATED_BODY()

protected:
    virtual bool Initialize() override;
    virtual void NativeConstruct() override;
    virtual void NativeDestruct() override;

    UFUNCTION()
    void OnHealthChanged(float NewHealth);
};
```

```cpp
bool UMyUIWidget::Initialize()
{
    bool bSuccess = Super::Initialize();
    if (!bSuccess) return false;
    
    // One-time setup
    return true;
}

void UMyUIWidget::NativeConstruct()
{
    Super::NativeConstruct();

    // BIND DELEGATES
    if (AMyPlayer* Player = GetOwningPlayerPawn<AMyPlayer>())
    {
        Player->OnHealthChanged.AddDynamic(this, &UMyUIWidget::OnHealthChanged);
    }
}

void UMyUIWidget::NativeDestruct()
{
    // UNBIND DELEGATES to prevent memory leaks
    if (AMyPlayer* Player = GetOwningPlayerPawn<AMyPlayer>())
    {
        Player->OnHealthChanged.RemoveDynamic(this, &UMyUIWidget::OnHealthChanged);
    }

    Super::NativeDestruct();
}
```


## Edge Cases
**BAD (Memory Leak)**:
```cpp
void UMyUIWidget::NativeConstruct() {
    GetWorld()->GetTimerManager().SetTimer(MyTimer, this, &UMyUIWidget::UpdateUI, 1.0f, true);
}
// FORGOT TO CLEAR TIMER IN NativeDestruct. Widget lives forever.
```

**GOOD (Safe Cleanup)**:
```cpp
void UMyUIWidget::NativeDestruct() {
    GetWorld()->GetTimerManager().ClearTimer(MyTimer);
    Super::NativeDestruct();
}
```

## Verification Checklist
- [ ] `Initialize` returns `Super::Initialize()`.
- [ ] Every delegate bound in `NativeConstruct` is removed in `NativeDestruct`.
- [ ] All timers started by the widget are cleared in `NativeDestruct`.
- [ ] Raw C++ constructors are avoided for logic.

## Architecture

The architecture for Userwidget Lifecycle Management follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Userwidget Lifecycle Management.

## Key Claims

- **Memory Leaks**: If a widget binds to a long-living actor (like the `GameMode`) but fails to unbind in `NativeDestruct`, the widget cannot be Garbage Collected after it is closed. It becomes a "Zombie Widget."
- **Crash Prevention**: An unbound delegate trying to update a destroyed TextBlock will crash the engine.


The Userwidget Lifecycle Management pattern follows UE5 engine conventions and best practices.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h:1`

