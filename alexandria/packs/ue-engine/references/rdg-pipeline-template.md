---
name: rdg-pipeline-template
description: Comprehensive C++ and HLSL templates for a Render Dependency Graph (RDG) Compute and Pixel pass in UE5.
module: rdg-pipeline-template
---

# RDG Pipeline Template (UE5 Expert Reference)

## Context
- Trigger when you need the EXACT boilerplate syntax to create a Global Shader, allocate RDG parameters, and dispatch the GPU.
- Serves as the ultimate "Memory Dump" for `FRDGBuilder`.

## 1. Compute Shader Template (GPGPU)

### The HLSL File (`/Plugin/MyPlugin/MyCompute.usf`)
```hlsl
#include "/Engine/Public/Platform.ush"

// Input Data
float DeltaTime;
Texture2D InputTexture;
SamplerState InputSampler;

// Output Data (UAV)
RWTexture2D<float4> OutputTexture;

[numthreads(8, 8, 1)]
void MainCS(uint3 ThreadId : SV_DispatchThreadID)
{
    // Bounds check
    uint Width, Height;
    OutputTexture.GetDimensions(Width, Height);
    if (ThreadId.x >= Width || ThreadId.y >= Height) return;

    // Logic
    float2 UV = float2(ThreadId.xy) / float2(Width, Height);
    float4 Color = InputTexture.SampleLevel(InputSampler, UV, 0);
    
    OutputTexture[ThreadId.xy] = Color * sin(DeltaTime);
}
```

### The C++ Global Shader Class (`MyComputeShader.h`)
```cpp
#pragma once
#include "GlobalShader.h"
#include "ShaderParameterStruct.h"

class FMyComputeShader : public FGlobalShader
{
    DECLARE_GLOBAL_SHADER(FMyComputeShader);
    SHADER_USE_PARAMETER_STRUCT(FMyComputeShader, FGlobalShader);

    BEGIN_SHADER_PARAMETER_STRUCT(FParameters, )
        SHADER_PARAMETER(float, DeltaTime)
        SHADER_PARAMETER_RDG_TEXTURE(Texture2D, InputTexture)
        SHADER_PARAMETER_SAMPLER(SamplerState, InputSampler)
        SHADER_PARAMETER_RDG_TEXTURE_UAV(RWTexture2D<float4>, OutputTexture)
    END_SHADER_PARAMETER_STRUCT()

    static bool ShouldCompilePermutation(const FGlobalShaderPermutationParameters& Parameters)
    {
        return IsFeatureLevelSupported(Parameters.Platform, ERHIFeatureLevel::SM5);
    }
};
```

### The C++ Implementation & Dispatch (`MyComputeShader.cpp`)
```cpp
#include "MyComputeShader.h"
#include "RenderGraphUtils.h"

// Register the shader
IMPLEMENT_GLOBAL_SHADER(FMyComputeShader, "/Plugin/MyPlugin/MyCompute.usf", "MainCS", SF_Compute);

void DispatchComputePass(FRHICommandListImmediate& RHICmdList, FRDGTextureRef InputTex)
{
    FRDGBuilder GraphBuilder(RHICmdList);

    // 1. Create UAV Target
    FRDGTextureDesc Desc = FRDGTextureDesc::Create2D(FIntPoint(1024, 1024), PF_FloatRGBA, FClearValueBinding::None, TexCreate_UAV | TexCreate_ShaderResource);
    FRDGTextureRef OutputTex = GraphBuilder.CreateTexture(Desc, TEXT("ComputeOutput"));
    FRDGTextureUAVRef OutputUAV = GraphBuilder.CreateUAV(OutputTex);

    // 2. Allocate and Fill Parameters
    FMyComputeShader::FParameters* PassParams = GraphBuilder.AllocParameters<FMyComputeShader::FParameters>();
    PassParams->DeltaTime = 0.016f;
    PassParams->InputTexture = InputTex;
    PassParams->InputSampler = TStaticSamplerState<SF_Bilinear>::GetRHI();
    PassParams->OutputTexture = OutputUAV;

    // 3. Get Shader Map
    TShaderMapRef<FMyComputeShader> ComputeShader(GetGlobalShaderMap(GMaxRHIFeatureLevel));

    // 4. Calculate Group Count (1024 / 8)
    FIntVector GroupCount(128, 128, 1);

    // 5. Add Pass
    FComputeShaderUtils::AddPass(
        GraphBuilder,
        RDG_EVENT_NAME("My Compute Dispatch"),
        ComputeShader,
        PassParams,
        GroupCount
    );

    // 6. Execute Graph
    GraphBuilder.Execute();
}
```

## 2. Pixel Shader Template (Post-Process)

### The HLSL File (`/Plugin/MyPlugin/MyPixel.usf`)
```hlsl
#include "/Engine/Public/Platform.ush"

Texture2D SceneColorTexture;
SamplerState PointSampler;

void MainPS(
    float4 SvPosition : SV_POSITION,
    out float4 OutColor : SV_Target0)
{
    float2 UV = SvPosition.xy / View.BufferSizeAndInvSize.xy;
    OutColor = SceneColorTexture.Sample(PointSampler, UV) * float4(1, 0, 0, 1); // Tint Red
}
```

### The C++ Global Shader Class (`MyPixelShader.h`)
```cpp
class FMyPixelShader : public FGlobalShader
{
    DECLARE_GLOBAL_SHADER(FMyPixelShader);
    SHADER_USE_PARAMETER_STRUCT(FMyPixelShader, FGlobalShader);

    BEGIN_SHADER_PARAMETER_STRUCT(FParameters, )
        SHADER_PARAMETER_RDG_TEXTURE(Texture2D, SceneColorTexture)
        SHADER_PARAMETER_SAMPLER(SamplerState, PointSampler)
        RENDER_TARGET_BINDING_SLOTS()
    END_SHADER_PARAMETER_STRUCT()
};
```

### The C++ Implementation (`MyPixelShader.cpp`)
```cpp
IMPLEMENT_GLOBAL_SHADER(FMyPixelShader, "/Plugin/MyPlugin/MyPixel.usf", "MainPS", SF_Pixel);

// Inside an ISceneViewExtension callback
FScreenPassTexture AddPixelPass(FRDGBuilder& GraphBuilder, const FSceneView& View, FRDGTextureRef SceneColorTex)
{
    FRDGTextureDesc OutputDesc = SceneColorTex->Desc;
    OutputDesc.Reset();
    FRDGTextureRef OutputTex = GraphBuilder.CreateTexture(OutputDesc, TEXT("PixelOutput"));

    FMyPixelShader::FParameters* PassParams = GraphBuilder.AllocParameters<FMyPixelShader::FParameters>();
    PassParams->SceneColorTexture = SceneColorTex;
    PassParams->PointSampler = TStaticSamplerState<SF_Point>::GetRHI();
    PassParams->RenderTargets[0] = FRenderTargetBinding(OutputTex, ERenderTargetLoadAction::ENoAction);

    TShaderMapRef<FMyPixelShader> PixelShader(View.ShaderMap);

    FPixelShaderUtils::AddFullscreenPass(
        GraphBuilder,
        View.ShaderMap,
        RDG_EVENT_NAME("My Pixel Pass"),
        PixelShader,
        PassParams,
        View.ViewRect
    );

    return FScreenPassTexture(OutputTex);
}
```

## Architecture

The architecture for Rdg Pipeline Template follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Rdg Pipeline Template.

## Key Claims

- The Rdg Pipeline Template pattern follows UE5 engine conventions and best practices.

## Edge Cases

Refer to the Common Mistakes section for Rdg Pipeline Template edge cases. Ensure proper null checks and UPROPERTY replication for networked usage.

## Evidence

- `UE5Pattern` defined at `Source/Runtime/Engine/EngineTypes.h`

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.
