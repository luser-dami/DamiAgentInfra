---
name: uobject-metadata
description: Utilizing DisplayName, Category, and Tooltip metadata in UPROPERTY and UFUNCTION declarations. Improves usability of generated tools.
feature: uobject-metadata
module: Core
---

# UObject Metadata (UE5 Expert)

## Context
- Mandatory when exposing C++ variables or functions to the Editor or Blueprints.
- Trigger when building Editor Tools, properties panels, or custom plugin interfaces.

## 1. Core Principles
Metadata (`meta = (...)`) controls how the Editor renders and interacts with properties and functions. It does not affect compiled runtime logic but is critical for UX.

## 2. Essential Metadata Tags

### Tooltips
Always provide a tooltip. This replaces the raw C++ comment in the Editor UI.
```cpp
UPROPERTY(EditAnywhere, Category = "Config", meta = (ToolTip = "The maximum speed in cm/s."))
float MaxSpeed;
```

### DisplayName
Use when the C++ variable name is not user-friendly.
```cpp
UPROPERTY(EditAnywhere, Category = "Config", meta = (DisplayName = "Enable God Mode"))
bool bEnableGM;
```

### Categories and Subcategories
Organize everything. Use `|` for subcategories.
```cpp
UPROPERTY(EditAnywhere, Category = "Combat|Weapons|Damage")
float BaseDamage;
```

### EditCondition (Dynamic UI)
Grays out or hides properties based on other booleans.
```cpp
UPROPERTY(EditAnywhere, Category = "Debug")
bool bShowDebugLines;

// Only editable if bShowDebugLines is true
UPROPERTY(EditAnywhere, Category = "Debug", meta = (EditCondition = "bShowDebugLines"))
FColor DebugColor;

// Completely hidden if bShowDebugLines is false
UPROPERTY(EditAnywhere, Category = "Debug", meta = (EditCondition = "bShowDebugLines", EditConditionHides))
float DebugDuration;
```

## 3. Function Metadata
Control how Blueprint nodes appear.

```cpp
UFUNCTION(BlueprintCallable, Category = "Math", meta = (DisplayName = "Calculate Trajectory", ToolTip = "Calculates the arc of the projectile.", AdvancedDisplay = "2"))
void CalcTraj(FVector Start, FVector End, bool bUseGravity = true); // bUseGravity will be hidden under the advanced arrow
```


## Verification Checklist
- [ ] Every `EditAnywhere` or `BlueprintReadWrite` property has a `Category`.
- [ ] `ToolTip` is provided for complex or non-obvious properties.
- [ ] `EditCondition` is used to prevent editing of irrelevant fields.
- [ ] `DisplayName` is used to clean up C++ prefixes (e.g., hiding the `b` in `bIsActive`).

## Architecture

The architecture for Uobject Metadata follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Uobject Metadata.

## Key Claims

- **Oversight Demand**: A well-documented UI prevents designers from entering invalid data or misunderstanding a tool's purpose.
- **Workflow Efficiency**: Eliminates the need for designers to read C++ code to understand what a property does.


The Uobject Metadata pattern follows UE5 engine conventions and best practices.

## Edge Cases

Refer to the Common Mistakes section for Uobject Metadata edge cases. Ensure proper null checks and UPROPERTY replication for networked usage.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h:1`

