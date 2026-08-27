---
name: automation-testing-framework
description: Writing C++ unit tests using IMPLEMENT_SIMPLE_AUTOMATION_TEST to validate agent-generated code.
feature: automation-testing-framework
module: Developer
---

# Automation Testing Framework (UE5 Expert)

## Context
- Mandatory when generating core mathematical libraries, inventory logic, or damage calculators.
- Trigger to ensure the agent's logic works without manually opening the Editor and pressing Play.

## 1. Core Principles
The Automation Framework allows you to write scripts that run in the `Session Frontend -> Automation` tab. They validate assumptions automatically.

## 2. Implementation Syntax

### The Test Declaration
Declared in a `.cpp` file (usually in a `Private/Tests/` folder).

```cpp
#include "Misc/AutomationTest.h"
#include "MyMathLibrary.h" // The class you want to test

// IMPLEMENT_SIMPLE_AUTOMATION_TEST(TestClass, "UI.Menu.Path", TestFlags)
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FMathCalcTest, "MyProject.Unit.Math.ComplexCalc", EAutomationTestFlags::ApplicationContextMask | EAutomationTestFlags::EngineFilter)

bool FMathCalcTest::RunTest(const FString& Parameters)
{
    // 1. Arrange
    float InputA = 5.0f;
    float InputB = 10.0f;

    // 2. Act
    float Result = UMyMathLibrary::CalculateComplex(InputA, InputB);

    // 3. Assert
    TestEqual(TEXT("CalculateComplex should return correct multiplication"), Result, 50.0f);
    
    // Test True/False
    TestTrue(TEXT("Result should be greater than 0"), Result > 0.0f);

    return true; // Test completion status
}
```

## 3. Complex Tests (World Required)
If you need to test Actors, use `IMPLEMENT_COMPLEX_AUTOMATION_TEST` or create a temporary world.

```cpp
// Create a hidden, temporary world for the test
UWorld* World = UWorld::CreateWorld(EWorldType::Game, false);
FWorldContext& WorldContext = GEngine->CreateNewWorldContext(EWorldType::Game);
WorldContext.SetCurrentWorld(World);

// Spawn Actor
AMyActor* TestActor = World->SpawnActor<AMyActor>();
TestNotNull(TEXT("Actor should spawn successfully"), TestActor);

// Teardown
World->DestroyWorld(false);
```


## Verification Checklist
- [ ] Tests use `IMPLEMENT_SIMPLE_AUTOMATION_TEST`.
- [ ] Flags include `EAutomationTestFlags::ApplicationContextMask` and `EngineFilter` (or `ProductFilter`).
- [ ] `TestEqual`, `TestTrue`, `TestNotNull` are used for assertions.
- [ ] Actors are spawned in a safely created and destroyed `UWorld` if testing in-game logic.

## Architecture

The architecture for Automation Testing Framework follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Automation Testing Framework.

## Key Claims

- **Regression Prevention**: Ensures that an agent refactoring a core system does not silently break 50 other systems that rely on it.
- **Continuous Integration**: These tests can be run via command line (`UnrealEditor-Cmd.exe -ExecCmds="Automation RunTests MyProject"`) on a build server.


The Automation Testing Framework pattern follows UE5 engine conventions and best practices.

## Edge Cases

Refer to the Common Mistakes section for Automation Testing Framework edge cases. Ensure proper null checks and UPROPERTY replication for networked usage.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h`

