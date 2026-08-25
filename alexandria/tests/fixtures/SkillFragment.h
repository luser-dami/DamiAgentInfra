// Copyright (c) 2026 luser-dami. MIT License.

#pragma once

#include "CoreMinimal.h"
#include "UObject/Object.h"

#include "SkillFragment.generated.h"

/**
 * GA 技能 Fragment 基类：一个 Fragment = 一个职责的纯数据配置。
 * 以固定命名槽位挂在 GA 基类上（非动态数组）：空槽 = 该功能关闭，
 * 策划在槽位上内联创建子对象即可，C++ 子类无需重复声明字段。
 */
UCLASS(Abstract, Blueprintable, EditInlineNew, DefaultToInstanced)
class SKILLRUNTIME_API USkillFragment : public UObject
{
	GENERATED_BODY()
};
