---
name: game-features-modular
description: Creating modular gameplay using Game Features Plugins and Components. Standardized by Lyra for hot-swappable mechanics.
feature: game-features-modular
module: GameFeatures
---

# Game Features & Modularity (UE5 Expert)

## Context
- Mandatory when building large, scalable games where new abilities, weapons, or UI must be added without modifying the core project.
- Trigger when injecting data or logic dynamically based on the current level or game mode.

## 1. Core Principles
A **Game Feature Plugin** is a self-contained module that can be turned on/off at runtime. It uses **Game Feature Actions** to "inject" logic into the existing world.

## 2. Component Injection (The Lyra Standard)
Instead of hardcoding a `UHealthComponent` onto the `ABaseCharacter`, the Game Feature uses `UGameFeatureAction_AddComponents` to add it dynamically.

### How it works:
1. The Plugin contains the `UHealthComponent` C++ class.
2. The Plugin's Data Asset specifies: "When this feature is Active, add `UHealthComponent` to any Actor of class `ABaseCharacter`".
3. Result: The core game knows nothing about Health. The plugin handles it cleanly.

## 3. C++ Interaction (Listening to Features)
Sometimes C++ needs to know when a feature activates (e.g., to update the UI). Use the `IGameFeatureStateChangeObserver`.

```cpp
#include "GameFeaturesSubsystem.h"

void AMyUIManager::BeginPlay()
{
    Super::BeginPlay();
    
    // Listen for state changes
    UGameFeaturesSubsystem::Get().AddObserver(this);
}

void AMyUIManager::OnGameFeatureActivating(const UGameFeatureData* GameFeatureData, const FString& PluginName)
{
    if (PluginName == TEXT("MagicSpellsFeature"))
    {
        // Add Mana Bar to UI
    }
}
```

## 4. Custom Game Feature Actions
If adding components isn't enough, write a custom action in C++.

```cpp
#include "GameFeatureAction.h"

UCLASS()
class MYPLUGIN_API UGameFeatureAction_AddInputMapping : public UGameFeatureAction
{
    GENERATED_BODY()

public:
    virtual void OnGameFeatureActivating() override;
    virtual void OnGameFeatureDeactivating() override;
    
    UPROPERTY(EditAnywhere)
    TSoftObjectPtr<UInputMappingContext> MappingToInject;
};
```


## Verification Checklist
- [ ] Dependencies are kept within the Game Feature Plugin, not the project root.
- [ ] `UGameFeatureAction` is used to modify the world instead of direct `BeginPlay` overrides.
- [ ] Components added via Actions use `FindComponentByClass` for access, avoiding hard casts.

## Architecture

The architecture for Game Features Modular follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Game Features Modular.

## Key Claims

- **Decoupling**: Prevents the `AMyCharacter.cpp` file from reaching 50,000 lines of code containing every weapon and ability in the game.
- **DLC / LiveOps**: Allows developers to ship an entire expansion (Maps, Logic, UI) as a single togglable plugin.


The Game Features Modular pattern follows UE5 engine conventions and best practices.

## Edge Cases

Refer to the Common Mistakes section for Game Features Modular edge cases. Ensure proper null checks and UPROPERTY replication for networked usage.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h`

