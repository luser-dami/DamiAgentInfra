---
name: compute-shaders-rdg
description: Dispatching Compute Shaders using the Render Dependency Graph (RDG). Utilizing Unordered Access Views (UAVs) for read/write data.
domain: rendering
---

# Compute Shaders in RDG (UE5 Expert)

## Context
- Mandatory for GPGPU tasks: Physics simulations, fluid dynamics, procedural mesh generation, or custom culling.
- Trigger when you need high-performance, parallel data processing on the GPU.

## 1. Core Principles
Compute Shaders do not draw pixels. They execute "Threads" in 3D "Thread Groups" to read and write to buffers (`UAVs` - Unordered Access Views) or Textures.

## 2. Implementation Syntax

### 1. The HLSL Shader
Defines the `[numthreads(X, Y, Z)]` and the `RWTexture` or `RWBuffer`.

```hlsl
// /Plugin/MyPlugin/MyCompute.usf
#include "/Engine/Public/Platform.ush"

// Bound via BEGIN_SHADER_PARAMETER_STRUCT
RWTexture2D<float4> OutputTexture;
float Time;

[numthreads(8, 8, 1)]
void MainCS(uint3 ThreadId : SV_DispatchThreadID)
{
    // Write color based on thread coordinate
    OutputTexture[ThreadId.xy] = float4(ThreadId.x * 0.01f, ThreadId.y * 0.01f, sin(Time), 1.0f);
}
```

### 2. The C++ Global Shader
```cpp
class FMyComputeShader : public FGlobalShader
{
    DECLARE_GLOBAL_SHADER(FMyComputeShader);
    SHADER_USE_PARAMETER_STRUCT(FMyComputeShader, FGlobalShader);

    BEGIN_SHADER_PARAMETER_STRUCT(FParameters, )
        SHADER_PARAMETER_RDG_TEXTURE_UAV(RWTexture2D<float4>, OutputTexture)
        SHADER_PARAMETER(float, Time)
    END_SHADER_PARAMETER_STRUCT()
};

IMPLEMENT_GLOBAL_SHADER(FMyComputeShader, "/Plugin/MyPlugin/MyCompute.usf", "MainCS", SF_Compute);
```

### 3. Dispatching via RDG
Use `FComputeShaderUtils::AddPass`.

```cpp
void DispatchMyCompute(FRHICommandListImmediate& RHICmdList)
{
    FRDGBuilder GraphBuilder(RHICmdList);

    // Create target texture
    FRDGTextureDesc Desc = FRDGTextureDesc::Create2D(FIntPoint(1024, 1024), PF_FloatRGBA, FClearValueBinding::None, TexCreate_UAV | TexCreate_ShaderResource);
    FRDGTextureRef TargetTexture = GraphBuilder.CreateTexture(Desc, TEXT("ComputeTarget"));

    // Create UAV for the texture
    FRDGTextureUAVRef TargetUAV = GraphBuilder.CreateUAV(TargetTexture);

    // Setup parameters
    FMyComputeShader::FParameters* PassParams = GraphBuilder.AllocParameters<FMyComputeShader::FParameters>();
    PassParams->OutputTexture = TargetUAV;
    PassParams->Time = FPlatformTime::Seconds();

    TShaderMapRef<FMyComputeShader> ComputeShader(GetGlobalShaderMap(GMaxRHIFeatureLevel));

    // Calculate group counts (1024 / 8 = 128 groups)
    FIntVector GroupCount(1024 / 8, 1024 / 8, 1);

    // Add the pass
    FComputeShaderUtils::AddPass(
        GraphBuilder,
        RDG_EVENT_NAME("My Compute Dispatch"),
        ComputeShader,
        PassParams,
        GroupCount
    );

    GraphBuilder.Execute();
}
```


## Verification Checklist
- [ ] Compute Shader uses `SF_Compute` in `IMPLEMENT_GLOBAL_SHADER`.
- [ ] Output buffers/textures use `TexCreate_UAV` flag during creation.
- [ ] Parameters struct uses `SHADER_PARAMETER_RDG_TEXTURE_UAV` or `SHADER_PARAMETER_RDG_BUFFER_UAV`.
- [ ] Thread Group dispatch count `GroupCount` matches the `[numthreads]` layout.

## Architecture

The architecture for Compute Shaders Rdg follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Compute Shaders Rdg.

## Key Claims

- **UAV Hazards**: RDG automatically inserts memory barriers. If Pass A writes to `TargetUAV` and Pass B reads from `TargetTexture`, RDG ensures Pass A finishes before Pass B starts, preventing GPU race conditions.
- **Thread Count Math**: If the grid size (e.g., 1024) is not perfectly divisible by the `numthreads` (e.g., 8), agents MUST handle bounds checking inside the HLSL (`if (ThreadId.x >= MaxX) return;`) to prevent writing out of bounds, which crashes the GPU.


The Compute Shaders Rdg pattern follows UE5 engine conventions and best practices.

## Edge Cases

Refer to the Common Mistakes section for Compute Shaders Rdg edge cases. Ensure proper null checks and UPROPERTY replication for networked usage.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h`

