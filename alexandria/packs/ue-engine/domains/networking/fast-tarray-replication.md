---
name: fast-tarray-replication
description: Using FFastArraySerializer for efficient network synchronization of large dynamic lists.
domain: networking
---

# Fast TArray Replication (UE5 Expert)

## Context
- Mandatory when replicating large or frequently changing dynamic arrays (e.g., Inventories, Buff/Debuff lists, Quest objectives).
- Trigger when a standard `UPROPERTY(Replicated) TArray<T>` causes massive bandwidth spikes.

## 1. The TArray Problem
A standard replicated `TArray` replicates the ENTIRE array every time ONE element is added, removed, or changed. If you have an inventory of 100 items and change the durability of item #4, all 100 items are sent over the network.

## 2. FFastArraySerializer (The Solution)
This struct allows the engine to replicate ONLY the specific elements that were added, removed, or modified.

### Step 1: Define the Item Struct
Must inherit from `FFastArraySerializerItem`.

```cpp
#pragma once
#include "Net/Serialization/FastArraySerializer.h"

USTRUCT()
struct FMyInventoryItem : public FFastArraySerializerItem
{
    GENERATED_BODY()

    UPROPERTY()
    int32 ItemID;

    UPROPERTY()
    float Durability;

    // Optional: PreReplicatedRemove, PostReplicatedAdd, PostReplicatedChange can be declared here
};
```

### Step 2: Define the Array Struct
Must inherit from `FFastArraySerializer` and contain a `TArray` of the items.

```cpp
USTRUCT()
struct FMyInventoryArray : public FFastArraySerializer
{
    GENERATED_BODY()

    UPROPERTY()
    TArray<FMyInventoryItem> Items;

    // Boilerplate required for FastArraySerializer
    bool NetDeltaSerialize(FNetDeltaSerializeInfo& DeltaParms)
    {
        return FFastArraySerializer::FastArrayDeltaSerialize<FMyInventoryItem, FMyInventoryArray>(Items, DeltaParms, *this);
    }
};

// Required trait for the struct
template<>
struct TStructOpsTypeTraits<FMyInventoryArray> : public TStructOpsTypeTraitsBase2<FMyInventoryArray>
{
    enum { WithNetDeltaSerializer = true };
};
```

### Step 3: Usage in Actor
Declare it and use the `MarkItemDirty` functions.

```cpp
UCLASS()
class AMyPlayerState : public APlayerState
{
    GENERATED_BODY()

protected:
    UPROPERTY(Replicated)
    FMyInventoryArray Inventory;
};

void AMyPlayerState::UpdateItemDurability(int32 Index, float NewDurability)
{
    if (Inventory.Items.IsValidIndex(Index))
    {
        Inventory.Items[Index].Durability = NewDurability;
        
        // ONLY replicates this specific item!
        Inventory.MarkItemDirty(Inventory.Items[Index]); 
    }
}

void AMyPlayerState::AddItem(int32 ID)
{
    FMyInventoryItem NewItem;
    NewItem.ItemID = ID;
    NewItem.Durability = 100.f;

    Inventory.Items.Add(NewItem);
    Inventory.MarkArrayDirty(); // Replicates the addition
}
```


## Verification Checklist
- [ ] Item struct inherits from `FFastArraySerializerItem`.
- [ ] Array struct inherits from `FFastArraySerializer`.
- [ ] `NetDeltaSerialize` and `TStructOpsTypeTraits` boilerplate are correctly implemented.
- [ ] `MarkItemDirty` is used when modifying existing elements.
- [ ] `MarkArrayDirty` is used when adding/removing elements.

## Architecture

The architecture for Fast Tarray Replication follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Fast Tarray Replication.

## Key Claims

- **Bandwidth**: Reduces payload from kilobytes to bytes per change.
- **Delegates**: `FFastArraySerializerItem` allows you to define `PostReplicatedAdd()` directly on the struct, allowing the client UI to react instantly when a new item arrives without writing complex array-diffing logic.


The Fast Tarray Replication pattern follows UE5 engine conventions and best practices.

## Edge Cases

Refer to the Common Mistakes section for Fast Tarray Replication edge cases. Ensure proper null checks and UPROPERTY replication for networked usage.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h`

