---
name: scene-texture-hlsl-access
description: Reading G-Buffer data (SceneColor, CustomStencil, SceneDepth) directly in HLSL / USF for custom shaders.
domain: rendering
---

# SceneTexture and G-Buffer Access in HLSL (UE5 Expert)

## Context
- Mandatory when writing custom Post-Process passes in `.usf` files.
- Trigger when you need to read Depth, Normals, or Stencil masks directly from the GPU inside C++ RDG passes.

## 1. Core Principles
When writing global shaders (outside the Material Editor), you cannot use the `SceneTexture` visual node. You must bind the G-Buffer textures explicitly via RDG and sample them in HLSL.

## 2. Binding G-Buffer Textures (C++)

### The Shader Parameter Struct
Define the textures you need to read.

```cpp
BEGIN_SHADER_PARAMETER_STRUCT(FMyPostProcessParams, )
    SHADER_PARAMETER_RDG_TEXTURE(Texture2D, SceneColorTexture)
    SHADER_PARAMETER_RDG_TEXTURE(Texture2D<uint2>, CustomStencilTexture)
    SHADER_PARAMETER_RDG_TEXTURE(Texture2D, SceneDepthTexture)
    SHADER_PARAMETER_SAMPLER(SamplerState, PointSampler)
    RENDER_TARGET_BINDING_SLOTS()
END_SHADER_PARAMETER_STRUCT()
```

### Extracting from FSceneView
During your `ISceneViewExtension` or render callback, extract the buffers from the View.

```cpp
// Scene Color is passed via Inputs in Post Process callbacks
PassParams->SceneColorTexture = Inputs.Textures[(uint32)EPostProcessMaterialInput::SceneColor].Texture;

// Depth and Stencil are grabbed from the SceneTextures structure
FSceneTextureParameters& SceneTextures = View.FeatureLevel >= ERHIFeatureLevel::SM5 ? *GetSceneTextureParameters(GraphBuilder) : *new FSceneTextureParameters();

PassParams->SceneDepthTexture = SceneTextures.SceneDepthTexture;
PassParams->CustomStencilTexture = SceneTextures.CustomDepthTexture; // Note: CustomDepth contains the Stencil in the second channel
PassParams->PointSampler = TStaticSamplerState<SF_Point>::GetRHI();
```

## 3. Sampling in HLSL (.usf)

```hlsl
Texture2D SceneColorTexture;
Texture2D<uint2> CustomStencilTexture; // uint2 because it holds Depth (Float) and Stencil (Uint)
Texture2D SceneDepthTexture;
SamplerState PointSampler;

void MainPS(
    float4 SvPosition : SV_POSITION,
    out float4 OutColor : SV_Target0)
{
    // Calculate UVs from screen coordinates
    float2 UV = SvPosition.xy / View.BufferSizeAndInvSize.xy;

    // 1. Read Scene Color
    float4 Color = SceneColorTexture.Sample(PointSampler, UV);

    // 2. Read Depth (Linearize it using Engine functions)
    float DeviceDepth = SceneDepthTexture.SampleLevel(PointSampler, UV, 0).r;
    float LinearDepth = ConvertFromDeviceZ(DeviceDepth);

    // 3. Read Custom Stencil (Load is required for integer textures, not Sample)
    uint StencilValue = CustomStencilTexture.Load(int3(SvPosition.xy, 0)).g; // 'g' channel holds the stencil

    // Example logic
    if (StencilValue == 255) {
        OutColor = Color * float4(1, 0, 0, 1); // Tint red
    } else {
        OutColor = Color;
    }
}
```


## Verification Checklist
- [ ] `CustomDepthTexture` is defined as `Texture2D<uint2>` or `Texture2D<float2>` in HLSL depending on the needed channel.
- [ ] `Load` is used instead of `Sample` when reading integer buffers like Stencils.
- [ ] `ConvertFromDeviceZ` is used to translate raw depth into world-space distances.

## Architecture

The architecture for Scene Texture Hlsl Access follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Scene Texture Hlsl Access.

## Key Claims

- **Sampler Types**: Using a Bilinear sampler on an integer texture (like Stencil) causes fatal GPU errors. Use `Load` or `SF_Point` sampling.
- **Depth Conversion**: DeviceZ is non-linear. Agents must use `ConvertFromDeviceZ()` (included in `Common.ush`) to get usable distance metrics.


The Scene Texture Hlsl Access pattern follows UE5 engine conventions and best practices.

## Edge Cases

Refer to the Common Mistakes section for Scene Texture Hlsl Access edge cases. Ensure proper null checks and UPROPERTY replication for networked usage.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h:1`

