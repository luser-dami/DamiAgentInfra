---
name: custom-asset-types
description: Creating new Asset Types in the Content Browser using FAssetTypeActions_Base and UFactory.
domain: editor
---

# Custom Asset Types (UE5 Expert)

## Context
- Mandatory when creating a new type of data file (e.g., a `.Quest` or `.Dialogue` file) that needs its own icon and double-click behavior.
- Trigger when `UPrimaryDataAsset` is insufficient because you need a bespoke Editor Window.

## 1. The Three Pillars
Creating a new asset requires three classes:
1. **The Object** (`UObject`): The actual runtime data container.
2. **The Factory** (`UFactory`): Tells the Editor *how* to create the object when you right-click -> "Create Advanced Asset".
3. **The Actions** (`FAssetTypeActions_Base`): Defines the color, icon, and what happens when you double-click it.

## 2. Implementation Syntax

### 1. The Factory
```cpp
#pragma once
#include "Factories/Factory.h"
#include "MyCustomAssetFactory.generated.h"

UCLASS()
class MYEDITOR_API UMyCustomAssetFactory : public UFactory
{
    GENERATED_BODY()
public:
    UMyCustomAssetFactory()
    {
        SupportedClass = UMyCustomAsset::StaticClass();
        bCreateNew = true;
        bEditAfterNew = true;
    }

    virtual UObject* FactoryCreateNew(UClass* Class, UObject* InParent, FName Name, EObjectFlags Flags, UObject* Context, FFeedbackContext* Warn) override
    {
        return NewObject<UMyCustomAsset>(InParent, Class, Name, Flags);
    }
};
```

### 2. The Asset Actions
```cpp
#include "AssetTypeActions_Base.h"

class FMyCustomAssetActions : public FAssetTypeActions_Base
{
public:
    virtual FText GetName() const override { return FText::FromString("My Custom Asset"); }
    virtual FColor GetTypeColor() const override { return FColor::Emerald; }
    virtual UClass* GetSupportedClass() const override { return UMyCustomAsset::StaticClass(); }
    virtual uint32 GetCategories() override { return EAssetTypeCategories::Misc; }
    
    // Override this if you want to open a custom Slate window on double-click
    // virtual void OpenAssetEditor(...) override; 
};
```

### 3. Registration
Register the actions in the Editor Module.

```cpp
#include "IAssetTools.h"
#include "AssetToolsModule.h"

TSharedPtr<FMyCustomAssetActions> AssetActions;

void FMyEditorModule::StartupModule()
{
    IAssetTools& AssetTools = FModuleManager::LoadModuleChecked<FAssetToolsModule>("AssetTools").Get();
    
    AssetActions = MakeShareable(new FMyCustomAssetActions());
    AssetTools.RegisterAssetTypeActions(AssetActions.ToSharedRef());
}

void FMyEditorModule::ShutdownModule()
{
    if (FModuleManager::Get().IsModuleLoaded("AssetTools"))
    {
        IAssetTools& AssetTools = FModuleManager::GetModuleChecked<FAssetToolsModule>("AssetTools").Get();
        AssetTools.UnregisterAssetTypeActions(AssetActions.ToSharedRef());
    }
}
```


## Verification Checklist
- [ ] The core `UObject` class is in a Runtime module.
- [ ] `UFactory` and `FAssetTypeActions` are in an Editor module.
- [ ] Actions are registered via `IAssetTools`.
- [ ] `GetTypeColor` returns a distinct color for UX clarity.

## Architecture

The architecture for Custom Asset Types follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Custom Asset Types.

## Key Claims

- **Clean Registry**: If you forget to `UnregisterAssetTypeActions` in `ShutdownModule`, hot-reloading the plugin will cause a crash or duplicate entries in the right-click menu.
- **Separation of Concerns**: The `UObject` data class must live in a **Runtime** module. The `UFactory` and `FAssetTypeActions` MUST live in an **Editor** module.


The Custom Asset Types pattern follows UE5 engine conventions and best practices.

## Edge Cases

Refer to the Common Mistakes section for Custom Asset Types edge cases. Ensure proper null checks and UPROPERTY replication for networked usage.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h`

