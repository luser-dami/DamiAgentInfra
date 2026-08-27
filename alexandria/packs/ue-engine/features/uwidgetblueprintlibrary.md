---
name: uwidgetblueprintlibrary
description: Leveraging C++ to manipulate widget trees at runtime. Dynamically generates UI elements based on live data or selection changes.
feature: uwidgetblueprintlibrary
module: UI
---

# UWidgetBlueprintLibrary (UE5 Expert)

## Context
- Mandatory when you need to create, manage, or find widgets dynamically via C++ or EditorUtilityBlueprints.
- Use for procedurally generating inventories, lists, or handling global UI state (like dragging).

## 1. Core Principles
`UWidgetBlueprintLibrary` is a static utility class that exposes powerful UMG functions to both C++ and Blueprints. It is the safest way to interact with the viewport and widget arrays.

## 2. Key Capabilities in C++

### Creating Widgets Dynamically
If you have a `TSubclassOf<UUserWidget>` (e.g., from a DataAsset), use `Create` to instantiate it.

```cpp
#include "Blueprint/WidgetBlueprintLibrary.h"

void AMyHUD::SpawnInventoryWidget(TSubclassOf<UUserWidget> InvClass)
{
    if (InvClass)
    {
        // Safe creation using the player controller as owner
        UUserWidget* InvWidget = UWidgetBlueprintLibrary::Create(this, InvClass, GetWorld()->GetFirstPlayerController());
        
        if (InvWidget)
        {
            InvWidget->AddToViewport();
        }
    }
}
```

### Finding All Widgets of a Class
Useful when you need to broadcast a global event to all open UI panels.

```cpp
void AMySystem::CloseAllMenus()
{
    TArray<UUserWidget*> FoundWidgets;
    UWidgetBlueprintLibrary::GetAllWidgetsOfClass(GetWorld(), FoundWidgets, UMyMenuWidget::StaticClass(), false);

    for (UUserWidget* Widget : FoundWidgets)
    {
        Widget->RemoveFromParent();
    }
}
```

### Input Mode Management
Essential for switching between Gameplay and UI.

```cpp
void AMyPlayerController::OpenMenu()
{
    // Show cursor and focus UI
    UWidgetBlueprintLibrary::SetInputMode_UIOnlyEx(this, MyMenuWidget, EMouseLockMode::DoNotLock, false);
    bShowMouseCursor = true;
}
```

## 3. Editor Utility Usage
When working in Editor Tooling, this library can be used to dismiss notifications or manage Drag & Drop operations within `EditorUtilityWidgets`.


## Edge Cases
**BAD (Manual Instantiation)**:
```cpp
UUserWidget* MyUI = NewObject<UUserWidget>(this, UIClass); // DANGEROUS: Lacks proper PlayerController initialization
MyUI->AddToViewport();
```

**GOOD (Library Instantiation)**:
```cpp
UUserWidget* MyUI = UWidgetBlueprintLibrary::Create(this, UIClass, PC); // SAFE
```

## Verification Checklist
- [ ] `UMG` is added to `PublicDependencyModuleNames` in `Build.cs`.
- [ ] `UWidgetBlueprintLibrary::Create` is used for dynamic UMG generation.
- [ ] `GetAllWidgetsOfClass` is used instead of manual array tracking where appropriate.
- [ ] Input mode is correctly restored (`SetInputMode_GameOnly`) when destroying UI.

## Architecture

The architecture for Uwidgetblueprintlibrary follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Uwidgetblueprintlibrary.

## Key Claims

- **Memory Safety**: `UWidgetBlueprintLibrary::Create` handles the correct instancing and PlayerController assignment, avoiding issues where widgets are spawned without an owning player (leading to input failures).
- **Iteration Avoidance**: Prevents the agent from attempting to manually iterate through `GEngine->GameViewport`, which is unsafe and error-prone.


The Uwidgetblueprintlibrary pattern follows UE5 engine conventions and best practices.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h`

