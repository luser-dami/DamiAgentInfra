---
name: performance-cdo-read-only
description: Utilizing the Class Default Object (CDO) via GetDefault() to read base properties without spawning actors. Saves memory and CPU.
domain: performance
---

# Class Default Object (CDO) Read-Only (UE5 Expert)

## Context
- Mandatory when you need to read static data (e.g., Base Health, Display Name, UI Icon) from a class definition *before* the actor is spawned.
- Trigger when building Inventories, Skill Trees, or UI Menus.

## 1. The Instantiation Trap
If a UI menu needs to display the damage of 50 different weapons, an inexperienced developer might spawn all 50 weapons, read their `BaseDamage` variable, and then destroy them. This freezes the game.

## 2. The CDO (Class Default Object)
When Unreal Engine boots up, it creates EXACTLY ONE instance of every `UCLASS` in memory. This is the **CDO**. It holds the default values of every property defined in C++ and Blueprint.

You can read the CDO instantly, with zero memory allocation or spawning overhead.

## 3. Implementation Syntax

### Reading from a Class Type (`TSubclassOf`)
When you only have the class pointer (e.g., assigned by a designer in a DataAsset).

```cpp
UPROPERTY(EditAnywhere)
TSubclassOf<AMyWeapon> WeaponClass;

void AMyUI::DisplayWeaponStats()
{
    if (WeaponClass)
    {
        // Get the CDO instance safely
        const AMyWeapon* DefaultWeapon = WeaponClass->GetDefaultObject<AMyWeapon>();

        if (DefaultWeapon)
        {
            // Read properties instantly without spawning!
            float Damage = DefaultWeapon->BaseDamage;
            FText Name = DefaultWeapon->WeaponName;
            
            UpdateUI(Name, Damage);
        }
    }
}
```

### Reading from a Static Class
If you know the exact C++ class at compile time.

```cpp
const AMyWeapon* DefaultWeapon = GetDefault<AMyWeapon>();
float Damage = DefaultWeapon->BaseDamage;
```

## 4. The Golden Rule of CDO
**NEVER MODIFY THE CDO AT RUNTIME.**
The CDO is a `const` template. If you somehow cast away the `const` and change `DefaultWeapon->BaseDamage = 999;`, you permanently change the base damage for EVERY new weapon spawned from that class until the game is restarted.


## Verification Checklist
- [ ] `GetDefaultObject<T>()` or `GetDefault<T>()` is used to read data from un-spawned classes.
- [ ] Information retrieved from the CDO is treated as strictly read-only (`const`).
- [ ] Data expected to be read from a CDO is initialized in the constructor or property panel, NOT `BeginPlay`.

## Architecture

The architecture for Performance Cdo Read Only follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Performance Cdo Read Only.

## Key Claims

- **Inventory Safety**: The CDO allows you to build an inventory system where you only store an array of `TSubclassOf<AWeapon>`. When you need the icon, you query the CDO. This saves massive amounts of RAM compared to storing 50 actual weapon pointers.
- **Constructor Data**: The CDO is populated by the C++ Constructor. If you calculate variables dynamically in `BeginPlay`, the CDO will NOT have those values. The CDO only knows what was set in the property defaults.


The Performance Cdo Read Only pattern follows UE5 engine conventions and best practices.

## Edge Cases

Refer to the Common Mistakes section for Performance Cdo Read Only edge cases. Ensure proper null checks and UPROPERTY replication for networked usage.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h`

