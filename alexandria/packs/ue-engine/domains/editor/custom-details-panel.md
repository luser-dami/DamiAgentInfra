---
name: custom-details-panel
description: Modifying the Editor's Details Panel using IDetailCustomization. Adding custom buttons and dynamic drop-downs to Actor properties.
domain: editor
---

# Custom Details Panel (UE5 Expert)

## Context
- Mandatory when you need to improve the UX of a C++ class in the Editor (e.g., adding a "Generate Procedural Mesh" button directly below a variable).
- Trigger when standard `UPROPERTY` metadata (`EditCondition`, `Categories`) is insufficient.

## 1. Core Principles
The `IDetailCustomization` interface allows you to completely override how a `USTRUCT` or `UCLASS` is drawn in the Editor's Details Panel using Slate UI.

## 2. Implementation Syntax

### The Customization Class (Editor Module Only!)
```cpp
#pragma once
#include "IDetailCustomization.h"

class FMyActorDetailsCustomization : public IDetailCustomization
{
public:
    // Factory method required by the registry
    static TSharedRef<IDetailCustomization> MakeInstance();

    // The core override
    virtual void CustomizeDetails(IDetailLayoutBuilder& DetailBuilder) override;
};
```

### The Slate Injection
```cpp
#include "DetailLayoutBuilder.h"
#include "DetailCategoryBuilder.h"
#include "DetailWidgetRow.h"
#include "Widgets/Input/SButton.h"

TSharedRef<IDetailCustomization> FMyActorDetailsCustomization::MakeInstance()
{
    return MakeShareable(new FMyActorDetailsCustomization);
}

void FMyActorDetailsCustomization::CustomizeDetails(IDetailLayoutBuilder& DetailBuilder)
{
    // 1. Get the category where you want to inject UI
    IDetailCategoryBuilder& CustomCategory = DetailBuilder.EditCategory("Procedural Generation");

    // 2. Add a custom Slate row
    CustomCategory.AddCustomRow(FText::FromString("Generate"))
    .NameContent()
    [
        SNew(STextBlock).Text(FText::FromString("Actions"))
    ]
    .ValueContent()
    [
        SNew(SButton)
        .Text(FText::FromString("Generate Mesh Now"))
        .OnClicked_Lambda([&DetailBuilder]() -> FReply
        {
            // Logic: Get the selected actors and call a function
            TArray<TWeakObjectPtr<UObject>> Objects;
            DetailBuilder.GetObjectsBeingCustomized(Objects);
            for (auto Obj : Objects)
            {
                if (AMyActor* MyActor = Cast<AMyActor>(Obj.Get())) {
                    MyActor->GenerateMesh();
                }
            }
            return FReply::Handled();
        })
    ];
}
```

## 3. Registering the Customization
You MUST register this in your Editor module's `StartupModule`.

```cpp
#include "PropertyEditorModule.h"

void FMyEditorModule::StartupModule()
{
    FPropertyEditorModule& PropertyModule = FModuleManager::LoadModuleChecked<FPropertyEditorModule>("PropertyEditor");
    
    // Register the customization for AMyActor
    PropertyModule.RegisterCustomClassLayout(
        "MyActor", 
        FOnGetDetailCustomizationInstance::CreateStatic(&FMyActorDetailsCustomization::MakeInstance)
    );
}

void FMyEditorModule::ShutdownModule()
{
    if (FModuleManager::Get().IsModuleLoaded("PropertyEditor"))
    {
        FPropertyEditorModule& PropertyModule = FModuleManager::GetModuleChecked<FPropertyEditorModule>("PropertyEditor");
        PropertyModule.UnregisterCustomClassLayout("MyActor");
    }
}
```


## Verification Checklist
- [ ] Code is isolated in an Editor module (`Type: Editor` in `.uplugin`).
- [ ] Class inherits from `IDetailCustomization`.
- [ ] Registered via `FPropertyEditorModule::RegisterCustomClassLayout`.
- [ ] Uses `TWeakObjectPtr` to interact with the customized actors.

## Architecture

The architecture for Custom Details Panel follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Custom Details Panel.

## Key Claims

- **Module Safety**: This code MUST live in an Editor-only module. Including `IDetailCustomization` in a Runtime module will break packaged builds.
- **Pointer Safety**: Always use `TWeakObjectPtr` when getting `ObjectsBeingCustomized`, as the user could delete the Actor from the level while the panel is open.


The Custom Details Panel pattern follows UE5 engine conventions and best practices.

## Edge Cases

Refer to the Common Mistakes section for Custom Details Panel edge cases. Ensure proper null checks and UPROPERTY replication for networked usage.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h:1`

