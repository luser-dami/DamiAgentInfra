---
name: bindwidget
description: Linking C++ variables to UMG components using meta=(BindWidget). Ensures UI logic stays in C++ while layout remains flexible in BP.
feature: bindwidget
module: UI
---

# BindWidget (UE5 Expert)

## Context
- Mandatory when writing C++ logic for a UMG UserWidget.
- Trigger when you need to access UI elements (Buttons, TextBlocks, ProgressBars) from C++.

## 1. Core Principles
`BindWidget` forces the Blueprint designer to include a widget of the EXACT same name and type as the C++ variable. It links the visual element to the C++ logic at compile time.

## 2. Implementation Syntax

### Header (.h)
```cpp
#pragma once
#include "CoreMinimal.h"
#include "Blueprint/UserWidget.h"
#include "MyUIWidget.generated.h"

class UTextBlock;
class UButton;

UCLASS()
class MYPROJECT_API UMyUIWidget : public UUserWidget
{
    GENERATED_BODY()

protected:
    // MUST match the name in the UMG Hierarchy exactly
    UPROPERTY(meta = (BindWidget))
    UTextBlock* PlayerNameText;

    UPROPERTY(meta = (BindWidget))
    UButton* SubmitButton;

    // Use Optional if the widget is not strictly required
    UPROPERTY(meta = (BindWidgetOptional))
    UTextBlock* OptionalWarningText;

    virtual void NativeConstruct() override;

    UFUNCTION()
    void OnSubmitClicked();
};
```

### Source (.cpp)
```cpp
#include "Components/TextBlock.h"
#include "Components/Button.h"

void UMyUIWidget::NativeConstruct()
{
    Super::NativeConstruct();

    // Bind the button click to our C++ function
    if (SubmitButton)
    {
        SubmitButton->OnClicked.AddDynamic(this, &UMyUIWidget::OnSubmitClicked);
    }
}

void UMyUIWidget::OnSubmitClicked()
{
    PlayerNameText->SetText(FText::FromString("Submitted!"));
}
```


## Edge Cases
**BAD (Getting Widget by Name at Runtime)**:
```cpp
UTextBlock* NameText = Cast<UTextBlock>(GetWidgetFromName(TEXT("PlayerNameText"))); // SLOW and unsafe!
```

**GOOD (BindWidget)**:
```cpp
UPROPERTY(meta = (BindWidget))
UTextBlock* PlayerNameText; // Linked automatically at compile time.
```

**BAD (Unsafe Binding)**:
```cpp
void UMyUIWidget::NativeConstruct() {
    SubmitButton->OnClicked.AddDynamic(...); // CRASH if BindWidgetOptional was used and button is null
}
```

**GOOD (Safe Binding)**:
```cpp
void UMyUIWidget::NativeConstruct() {
    if (SubmitButton) { SubmitButton->OnClicked.AddDynamic(...); }
}
```

## Verification Checklist
- [ ] Variable names match the UMG Hierarchy exactly.
- [ ] Forward declarations (`class UButton;`) are used in the header.
- [ ] `BindWidgetOptional` is used ONLY if the UI can function without the element.
- [ ] Dynamic delegates (`OnClicked.AddDynamic`) are bound safely in `NativeConstruct`.

## Architecture

The architecture for Bindwidget follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Bindwidget.

## Key Claims

- **Compile-Time Enforcement**: If the Blueprint designer deletes `PlayerNameText` or misspells it, the Blueprint WILL NOT COMPILE. This prevents silent runtime null-pointer crashes.
- **Performance**: Keeps expensive UI update logic (like calculating health percentages every frame) in native C++ rather than Blueprint visual scripts.


The Bindwidget pattern follows UE5 engine conventions and best practices.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h:1`

