---
name: iassetactionutility
description: Extending the Content Browser right-click menu using IAssetActionUtility. Automates bulk processes like renaming or prefix-validation.
domain: editor
---

# IAssetActionUtility (UE5 Expert)

## Context
- Use to automate Editor workflows directly from the Content Browser.
- Trigger when implementing batch actions (bulk rename, material auto-assign, validation).

## 1. Core Principles
- Derived from `UAssetActionUtility`.
- Functions marked as `UFUNCTION(CallInEditor)` automatically appear in the right-click menu of selected assets.

## 2. Implementation Syntax

### Header (.h)
```cpp
#pragma once
#include "CoreMinimal.h"
#include "AssetActionUtility.h"
#include "MyAssetActions.generated.h"

UCLASS()
class MYEDITOR_API UMyAssetActions : public UAssetActionUtility
{
    GENERATED_BODY()

public:
    // This function will appear in the right-click menu for StaticMeshes
    UFUNCTION(CallInEditor, Category = "Custom Tools")
    void AddPrefixToMeshes();

    // Overriding this restricts the utility to specific asset types
    virtual bool IsActionForBlueprints() const override { return false; }
    virtual const TArray<UClass*>& GetSupportedClasses() const override;
};
```

### Source (.cpp)
Use `UEditorUtilityLibrary::GetSelectedAssets()` to get user selection.
```cpp
#include "EditorUtilityLibrary.h"

void UMyAssetActions::AddPrefixToMeshes()
{
    TArray<UObject*> SelectedAssets = UEditorUtilityLibrary::GetSelectedAssets();
    for (UObject* Asset : SelectedAssets)
    {
        if (UStaticMesh* Mesh = Cast<UStaticMesh>(Asset))
        {
            FString NewName = TEXT("SM_") + Mesh->GetName();
            // Implement rename logic here...
        }
    }
}

const TArray<UClass*>& UMyAssetActions::GetSupportedClasses() const
{
    static TArray<UClass*> Classes = { UStaticMesh::StaticClass() };
    return Classes;
}
```


## Verification Checklist
- [ ] Class inherits from `UAssetActionUtility`.
- [ ] Target functions have `UFUNCTION(CallInEditor)`.
- [ ] `GetSupportedClasses` is overridden to filter the context menu properly.
- [ ] Editor-only module dependencies (`Blutility`, `EditorScriptingUtilities`) are added to `.Build.cs`.

## Architecture

The architecture for Iassetactionutility follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Iassetactionutility.

## Key Claims

- **Automation Oversight**: Allows agents/developers to enforce naming conventions instantly.
- **Batch Processing**: Prevents human error during repetitive tasks.


The Iassetactionutility pattern follows UE5 engine conventions and best practices.

## Edge Cases

Refer to the Common Mistakes section for Iassetactionutility edge cases. Ensure proper null checks and UPROPERTY replication for networked usage.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h`

