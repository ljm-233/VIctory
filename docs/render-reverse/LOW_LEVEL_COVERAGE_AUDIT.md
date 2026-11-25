# 底层渲染调用交叉引用补查

本次不只复查已有伪代码，而是按导入函数、IID、函数表位移和调用者重新扫描。`dll/XboxBvhBuilder.dll` 的导出实现用于验证 BVH 语义，当前 EXE 仍是调用事实的权威。

## 新确认的遗漏

| 类别 | 当前 EXE 证据 | 结论 |
|---|---|---|
| 根签名序列化 | `0x14247FC80`、`0x1455AC80A` | 两条 `D3D12SerializeVersionedRootSignature` 路径；空 local root signature 和完整参数序列化，blob 必须释放 |
| 纹理 swizzle | `0x140C4B3CE`、`0x144ECCCA2` | 资源描述会调用 `XGSuggestSwizzleMode`；强制兼容时返回 `64KB_D=266`，默认哨兵为 `0xFF` |
| Query heap | `0x142481B08`、`0x142481B33` | `CreateQueryHeap`，随后另有资源分配信息查询，不能只跟踪 Draw/Dispatch |
| CommandSignature | `0x14247953C..0x14247962A` | 初始化创建 5 个签名：Draw、DrawIndexed、DispatchL2、Dispatch、DispatchMesh |
| ExecuteIndirect | `0x14248CA90`、`0x14248CBE0`、`0x14248CD20`、`0x14248D000`、`0x14248D1B0`、`0x14248D350`、`0x14248D500`、`0x140094360`、`0x144E33E10` | 9 个实际包装、5 个签名；GCL slot 61 / `0x1E8`，Draw/DrawIndexed/Dispatch/DispatchL2/DispatchMesh 和独立/共享参数均已区分 |
| DXR StateObject | `0x1424801B0`、`0x1424813F9`、`0x142481427` | N+9 个子对象和关联已恢复；CreateStateObject 后再 QueryInterface Properties，identifier 指针依赖对象存活 |
| BVH 批量构建 | `0x140FE4E50`、`0x142490110`、`0x140FE4FEC`、`0x140FE5221`、`0x140FE5352`、`0x140FE5500` | 80 字节描述，最多 16 项；update/scratch/compaction 与两条路径已展开，barrier 不再误写成固定每批一次 |
| BVH COMPACT 拷贝 | `0x140BD9709` | 每对目的/源地址调用 `CopyRaytracingAccelerationStructure(...,COMPACT)` |
| BVH 重水合 | `0x140EA6D5A`、`0x140EA6E7C` | 最多 16 个描述一批，携带原始 instance data 地址 |
| BVH PrebuildInfo | `0x14007595C`、`0x144EE2B31`、`0x1456AC171` | 三条 PrebuildInfo 路径；输出 ResultDataMaxSize 和 ScratchDataSize |
| Morton 模式 | 同上及 `0x140FE51DB/0x140FE5576` | 使用内联 `Set64BitMortonCodes`，构建后恢复 false；不是 EXE 导入函数 |
| 物理页 scatter | `0x140B18DDE`、`0x140F2CC49` | `XMemMapPhysicalPagesScatter` 与页索引映射并存；后者还调用 queue `CopyPageMappingsBatchX` |
| 工具内存迁移 | `0x14221608E` | `XMemTransferMemory(XMemTransferToolsToSystem,...)` 按 2MiB 对齐，并以 16MiB 为上限；迁移量取 `min(AlignUp(ToolsLimit-ToolsUsed,2MiB),16MiB)`，KernelPool 字段只控制分支 |
| GPU hang 调试 | `0x1424745CF`、`0x1424745E4`、`0x14247464E` | `SetHangCallbacksX`、`ReportGpuHaltX(3)`、`AddBlobToGpuHangDumpX` 形成独立错误链 |

## DLL 交叉验证

`dll/XboxBvhBuilder.dll` 导出实现确认：

- `BuildRaytracingAccelerationStructureBatched2X`（DLL `0x1800216A0`）会验证 build desc 和 postbuild 信息，dest 地址必须 256 字节对齐。
- COMPACT copy 只处理对应模式。
- `Set64BitMortonCodes` 是布尔全局模式开关，支持构建前设置、完成后恢复。
- PrebuildInfo 的空指针检查、输入和 extra options 布局与当前调用形状一致。
- 批量构建内部还包含验证和资源同步，不应把 BVH 简化成一次普通 Draw/Dispatch。

该 DLL 没有提供 XS PSO 的 32 字节快路径生成器。`NativePsoInlineData` 的布局和使用已恢复；私有生成算法属于运行库内部优化边界，标准后端走 `SetPipelineState`，不构成当前 title 的功能缺口。

## 仍未完全展开

- StateObject 的 N+9 个子对象、export 去重和 association 已恢复；异步任务返回 1 仍只表示已接收，需要单独等待对象就绪。
- 9 个 ExecuteIndirect 包装已确认签名和参数；DispatchL2 的直接 flags 为 NONE，当前 title 可静态映射到标准 Dispatch，原始游戏 pass 名未全部确认。
- BVH rehydrate 已确认标准 56 字节序列化头；compacted-size 的 5/24 已映射为 UAV/COPY_SOURCE，复制方向为全局 buffer → build-info resource，详见 `ADVANCED_RENDER_INTERFACES.md`。
- **执行证据边界**：StateObject 创建已恢复；全 `.text` 审计未证实 SetPipelineState1、shader binding table、DispatchRays/DispatchRaysX 的实际执行，不虚构 SBT。标准 DispatchMesh 与私有 DispatchMeshX 同样未证实 direct 调用，AS/MS PSO 创建绑定已恢复；QueryHeap 的 Begin/End/Resolve 按实际命中区分，分类见 `RENDER_REMAINING_AUDIT.md`。
- 物理页 class/tag、部分 XMem 属性值继续保留观察值，不映射成未公开枚举。
- 动态 FidelityFX loader 的完整使用链不在本次 D3D 主路径内；提供的 upscaler DLL 有 `ffxCreateContext/ffxDispatch/ffxQuery` 导出，但不能仅凭 DLL 存在就认定游戏当前帧一定调用。

## 覆盖边界

本次补查发现的底层遗漏已补入 `pseudocode/09_low_level_calls.cpp`；后续逐地址复核又修正了间接包装映射、rehydrate 地址、Morton scratch 和迁移上限，并补齐 packed page commands、packed geometry/instance 和完整 root-signature bitstream。函数表位移扫描存在大量同偏移的非 D3D 对象，因此只把有 IID、导入调用或明确对象来源的候选升级为事实，没有把全部 `call [reg+offset]` 都误标为 D3D 调用。
