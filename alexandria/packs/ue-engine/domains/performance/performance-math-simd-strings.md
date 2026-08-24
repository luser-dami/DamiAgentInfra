---
name: performance-math-simd-strings
description: Mathematical optimizations in C++. Banning SQRT in hot loops, utilizing DistSquared, and replacing FString concatenations with FStringBuilderBase.
domain: performance
---

# Math & String Optimization (UE5 Expert)

## Context
- Mandatory when writing logic inside `Tick`, nested `for` loops, or complex algorithms (Pathfinding, AI, Procedural Generation).
- Trigger when comparing distances or building log/UI strings dynamically.

## 1. Math Optimizations

### Avoid Square Roots (DistSquared)
`FMath::Sqrt` is one of the most expensive CPU operations. 
When comparing distances (e.g., "Is the enemy closer than 500 units?"), you normally calculate the distance (which uses a square root internally). Instead, compare the **Squared Distances**.

**BAD (Square Root Overhead)**:
```cpp
if (FVector::Distance(PlayerLoc, EnemyLoc) < 500.0f) { ... }
```

**GOOD (Lightning Fast)**:
```cpp
// 500 squared is 250,000. Calculate it once or let the compiler do it.
if (FVector::DistSquared(PlayerLoc, EnemyLoc) < (500.0f * 500.0f)) { ... }
```

### Division vs Multiplication
Division (`/`) is mathematically slower for the CPU than multiplication (`*`).

**BAD (Division)**:
```cpp
float HalfHealth = Health / 2.0f;
```

**GOOD (Multiplication)**:
```cpp
float HalfHealth = Health * 0.5f;
```

### SIMD (Single Instruction, Multiple Data)
Unreal's math library automatically vectorizes math. But you must use the correct types.
Use `FVector4f` (or `FVector` in UE5, which is `double`) rather than doing math on individual floats when applying transforms, as the CPU can calculate all X, Y, Z axes in a single clock cycle.

## 2. String Optimizations

### FStringBuilderBase (No More Allocations)
Every time you use `+=` on an `FString`, or use `FString::Printf`, you ask the OS to allocate a new, larger block of memory, copy the string over, and delete the old one. Doing this in a loop destroys performance.

Use `FStringBuilderBase` (specifically `TStringBuilder<256>`) to allocate a fixed block of memory on the stack and write text directly into it.

**BAD (Memory Thrashing)**:
```cpp
FString FinalLog = "Results: ";
for (int32 i = 0; i < 100; ++i) {
    FinalLog += FString::FromInt(i) + ", "; // 100 re-allocations!
}
```

**GOOD (Zero Heap Allocations)**:
```cpp
TStringBuilder<256> Builder;
Builder.Append(TEXT("Results: "));
for (int32 i = 0; i < 100; ++i) {
    Builder.Appendf(TEXT("%d, "), i); // Writes directly into the pre-allocated stack buffer
}
FString FinalLog = Builder.ToString();
```


## Verification Checklist
- [ ] `DistSquared` or `SizeSquared` is used instead of `Distance` or `Size` for threshold comparisons.
- [ ] Multiplication by a fraction (`* 0.5f`) is used instead of division by an integer (`/ 2`).
- [ ] `TStringBuilder<N>` is used for complex string assembly instead of `FString +=`.

## Architecture

The architecture for Performance Math Simd Strings follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Performance Math Simd Strings.

## Key Claims

- **Float vs Double**: In UE5, `FVector` uses `double` (64-bit precision) by default to support massive open worlds. If you pass an `FVector` to an old function expecting `float`, you will lose precision. Use `FVector3f` explicitly if you want 32-bit floats.
- **Log Spam**: Building strings is expensive. Even with `FStringBuilder`, never build complex strings inside `Tick` unless actively displaying debug data.


The Performance Math Simd Strings pattern follows UE5 engine conventions and best practices.

## Edge Cases

Refer to the Common Mistakes section for Performance Math Simd Strings edge cases. Ensure proper null checks and UPROPERTY replication for networked usage.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h:1`

