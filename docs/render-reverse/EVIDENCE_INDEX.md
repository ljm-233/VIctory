# 渲染层证据索引

状态含义：`reconstructed` = 已写成语义化伪代码并复核主要分支；`partial` = 仅确认部分职责/字段；`unresolved` = 不能可靠命名。

## 已定位的核心对象和函数

| 地址 | 当前语义命名 | 状态 | 主要证据 | 对应输出 |
|---|---|---|---|---|
| `0x1419F4DF0` | `Platform_GraphicsBoot` | partial | 初始化图形全局状态并调用设备创建 | `01_platform_device.cpp` |
| `0x142478440` | `Platform_CreateWindowAndD3D12Device` | partial | 创建 Device11、1 direct + 4 compute queue、11 个列表槽、descriptor/PSO 等 | `01_platform_device.cpp` |
| `0x140188430` | `RenderThreadContext_AcquireCommandList` | partial | TLS 上下文、队列上下文关联、从 direct/compute 池取列表 | `01_platform_device.cpp` |
| `0x142474BE0` | `DirectCommandListPool_Acquire` | partial | 池命中或返回空 | `01_platform_device.cpp` |
| `0x142474C50` | `ComputeCommandListPool_Acquire` | partial | 按队列上下文取列表 | `01_platform_device.cpp` |
| `0x142474CC0` | `CommandList_ResetX` | reconstructed | GCL `+0x458`，四参 `(list,allocator,nullptr,none)` | `01_platform_device.cpp` |
| `0x14248CE00` | `RenderThreadContext_DrawIndexed` | reconstructed | 合并 dirty 状态，24 字节 DrawIndexed 包，慢路径 slot 13 | `05_command_recording.cpp` |
| `0x140032520` | `RenderThreadContext_FlushFullState` | reconstructed | topology、PSO、IA、RT/DS、viewport/scissor、root，以及单记录/全不同/混合分块 transition | `05_command_recording.cpp` |
| `0x140032130` | `RenderThreadContext_FlushRootState` | reconstructed | root table/SRV/UAV/constants 的快慢路径 | `05_command_recording.cpp` |
| `0x1400320E0` | `RenderThreadContext_FlushPendingBarriers` | reconstructed | GCL146 Barrier(groupCount,groups)，清空 pending 状态 | `05_command_recording.cpp` |
| `0x1400095E0` | `PendingBarrierSet_AppendBufferBarrier` | reconstructed | 16 字节 group、40 字节 buffer barrier、容量检查 | `05_command_recording.cpp` |
| `0x1400097F0` | `PendingBarrierSet_AppendTextureBarrier` | reconstructed | 64 字节 texture barrier、全子资源默认范围 | `05_command_recording.cpp` |
| `0x1400099F0` | `PendingBarrierSet_AppendTextureBarrierRange` | reconstructed | 64 字节 texture barrier、mip/array/plane 范围 | `05_command_recording.cpp` |
| `0x144D9EFD0` | `PopulateBindingDescriptors` | reconstructed | binding id 位段、默认描述符、32 字节描述符复制 | `05_command_recording.cpp` |
| `0x1424846D0` | `Platform_FrameStep` | partial | 消息泵、视图资源、提交 Present、推进帧、回收延迟对象 | `02_frame_submission.cpp` |
| `0x140CD72B0/0x140CD7320` | `Platform_EnqueuePresent` | reconstructed | 内联 1–2 个 plane、估计 FPS、显示间隔、完整记录字段和 frame token | `02_frame_submission.cpp` |
| `0x1401F6330` | `FrameSubmissionQueue_Produce` | reconstructed | 96 项环形批次和计数信号量；free state 初始高 32 位为 96 | `02_frame_submission.cpp` |
| `0x140006180/0x1400062D0` | `PackedSemaphore_Wait/Release` | reconstructed | 低 7 位等待 token、bits 7..30 自旋预算、高 32 位计数 | `02_frame_submission.cpp` |
| `0x142481E90` | `FrameSubmissionQueue_Consume` | reconstructed | 80 字节记录、共用 +0x28 冲刷标志、内联 Present、批量 Execute、Signal | `02_frame_submission.cpp` |
| `0x14247AB10` | `ApplyDisplayTiming` | reconstructed | 60/120Hz 间隔表、自动 1..4 倍率、SetFrameIntervalX 和 ScheduleFrameEventX | `02_frame_submission.cpp` |
| `0x142482660` | `FrameSubmissionQueue_ExecutePooledList` | reconstructed | 单列表 `ExecuteCommandLists2X`，flags 为 none | `02_frame_submission.cpp` |
| `0x142474740` | `Platform_AdvanceFrameIndex` | partial | 双槽帧对象、循环视图索引、12 位环形延迟回调索引 | `02_frame_submission.cpp` |
| `0x14248B570` | `Platform_GraphicsShutdown` | reconstructed | 空 Present flush、停止消费者、释放池/队列/device/分配域 | `08_suspend_shutdown.cpp` |
| `0x142490080` | `Platform_SuspendAndResumeGraphics` | reconstructed | SuspendX/ResumeX、显示时序和 DISPLAY_FLIP frame event | `08_suspend_shutdown.cpp` |
| `0x141E1CF60` | `GraphicsVirtualMemory_ConfigureOnce` | partial | VA 配置、两段地址保留、池变体初始化 | `03_resource_memory.cpp` |
| `0x141E86440` | `StagedMemoryRegions_Initialize` | partial | 多区域 VA/物理页映射、placed upload resource | `03_resource_memory.cpp` |
| `0x141E6C110` | `PhysicalPagePool_Initialize` | partial | 64KiB 页、21 位索引、每项 3 个打包、page-pool 注册 | `03_resource_memory.cpp` |
| `0x142474690` | `AllocateResourceAddress` | reconstructed | 128 字节对齐、按 subtype 选择分配器 | `03_resource_memory.cpp` |
| `0x1401086B0` | `ReleaseResourceAddress` | reconstructed | 先线程 arena，再按 +0x10 低 2 位 subtype/allocatorClass 释放 | `03_resource_memory.cpp` |
| `0x14193DE10` | `CreateRageResource` | partial | committed/placed/backing-only、初始上传、失败回收 | `03_resource_memory.cpp` |
| `0x1416F7100` | `UploadLinearTextureToTiledMemory` | reconstructed | XG address computer、plane/mip/slice、CopyIntoSubresource | `03_resource_memory.cpp` |
| `0x141CF3280` | `DescriptorHeap_Initialize` | reconstructed | CPU 区间、increment、GPU delta、external-CPU-base 标志 | `04_descriptors_copy.cpp` |
| `0x14156C240` | `DescriptorRangeAllocator_Allocate` | reconstructed | 原子对齐 bump 分配 | `04_descriptors_copy.cpp` |
| `0x1455AC240` | `DescriptorFreeList_Allocate` | reconstructed | 节点索引 CAS 和 CPU handle 地址 | `04_descriptors_copy.cpp` |
| `0x14006DC20` | `CreateRageBuffer` | partial | placed buffer、初始数据、原生/软件 CBV | `04_descriptors_copy.cpp` |
| `0x140CC2960` | `CreateRenderTargetView` | partial | RTV 维度/subresource；汇编确认 R9=handle | `04_descriptors_copy.cpp` |
| `0x1414A6940` | `CreateDepthStencilView` | partial | DSV 维度/只读 flags/subresource | `04_descriptors_copy.cpp` |
| `0x1404F4210` | `RecordResourceCopy` | partial | buffer/texture、整资源/区域、arena/独立 staging | `04_descriptors_copy.cpp` |
| `0x1424709C0` | `Gfx_InterfaceInit` | partial | 发布 25 项帧控制表、初始化 64 项 deferred 容量 | `06_frame_interface.cpp` |
| `0x142473400` | `DeferredFrameCallbackRegistry_Register` | reconstructed | 32 字节闭包、去重、压实追加 | `06_frame_interface.cpp` |
| `0x1424734C0` | `DeferredFrameCallbackRegistry_Unregister` | reconstructed | 同 key 删除并压实数组 | `06_frame_interface.cpp` |
| `0x142473560` | `FrameLifecycleCoordinator_EndFrameAndFlush` | reconstructed | scope/end-request、执行 deferred、资源销毁/重建 | `06_frame_interface.cpp` |
| `0x142473690` | `DestroyPresentationResources` | partial | 遍历 view 更新链，释放 SRV/UAV/RT/DS 与临时存储 | `06_frame_interface.cpp` |
| `0x142473850` | `RecreatePresentationResources` | partial | 每 view 重建资源、描述符和 RT/DS | `06_frame_interface.cpp` |
| `0x1421B7A40` | `Gfx_FlushDeferredResources` | reconstructed | 固定注册 48 个闭包，trampoline=sub_144FD1AB0；15 项具体命名、3 项无操作、30 项行为分类 | `06_frame_interface.cpp`、`deferred_callbacks.inc`、`evidence/deferred_callback_semantics.json` |
| `0x1423CF210` | `GameFrame_Update` | partial | pending → EndFrameAndFlush 的高层调用关系 | `06_frame_interface.cpp` |
| `0x140B5A5D0` | `GraphicsProgramCache_FindOrCreate` | reconstructed | 精确 v13..v20 hash、双表查找、插入、同步/异步创建 | `07_pipeline_programs.cpp` |
| `0x142479760` | `CompileGraphicsProgram` | reconstructed | 独立编译期 hash、stage/RT 掩码、派生缓存、688 字节 stream、Device86、200 KiB scratch | `07_pipeline_programs.cpp` |
| `0x14086F090/0x14086FA70` | `BuildGraphicsPipelineStateStream` | reconstructed | 15 个子对象的完整偏移、类型、payload 和外部 input-element 数组 | `07_pipeline_programs.cpp` |
| `0x142479F40` | `DerivedGraphicsPsoCache_FindOrDerive` | reconstructed | hash/base 匹配与 Device55 派生 | `07_pipeline_programs.cpp` |
| `0x14247FD30` | `CompileComputeProgram` | reconstructed | `a1[35]` 是 root signature、不是序列化 blob；`NodeMask=1`、序列化缓存优先、Device62 fallback，失败时 Release root | `07_pipeline_programs.cpp` |
| `0x142480050` | `SerializedComputePsoCache_FindAndDeserialize` | reconstructed | shader hash 查找与 Device67 反序列化 | `07_pipeline_programs.cpp` |
| `0x14086F8E0`、`0x1451189F0..0x145118B00`、`0x142157654` | Shader blob decode | reconstructed | 12 字节 `0xDCE7` 头、Stored/RawDeflate、总/输出大小和 scratch 生命周期已恢复 | `07_pipeline_programs.cpp`、`ADVANCED_RENDER_INTERFACES.md` |
| `0x14247FC40` | `CreateEmptyLocalRootSignature` | reconstructed | 空 local root signature、序列化与 blob 生命周期 | `09_low_level_calls.cpp` |
| `0x1455AC630` | `SerializePackedRootSignature` | reconstructed | 四类 root 参数、四组 range 位流、descriptor flags、静态 sampler、压缩 LOD 与末尾边界 | `09_low_level_calls.cpp` |
| `0x142481AA0` | `InitializeQueryObject` | reconstructed | CPU-only 数组、Device39、QueryHeap IID、结果 buffer 分配查询及失败释放 | `09_low_level_calls.cpp` |
| `0x1424794AC..0x142479650` | `CreateIndirectCommandSignatures` | reconstructed | 5 个签名：Draw、DrawIndexed、DispatchL2、Dispatch、DispatchMesh | `09_low_level_calls.cpp` |
| `0x14248CA90..0x14248D500`、`0x140094360`、`0x144E33E10` | `RecordExecuteIndirect` | reconstructed | 5 个签名、9 个包装，GCL61 / `0x1E8`，独立/共享参数缓冲和大小公式已恢复 | `09_low_level_calls.cpp`、`ADVANCED_RENDER_INTERFACES.md` |
| 标准 GCL `0x408`、私有 `0x320` 全 `.text` 审计 | Direct mesh execution audit | no confirmed direct execution | 337/34 个真实候选均未形成 direct mesh 证据；AS/MS PSO ready/bind 已恢复，间接 XYZ 为 12 字节 | `ADVANCED_RENDER_INTERFACES.md` 2.4 |
| `0x1424801B0` | `RaytracingPipeline_CreateStateObject` | reconstructed | N+9 个子对象、export/association、root signature、config 和 identifier 生命周期已恢复 | `09_low_level_calls.cpp`、`ADVANCED_RENDER_INTERFACES.md` |
| 主 GCL `0x3D0/0x3D8/0x430` 全 `.text` 审计、`0x1424785AD` | DXR execution audit | disabled by current device parameters | `DisableDXR=TRUE`；真实间接候选 42/28/31 均属于其他游戏接口，未证实 SetPipelineState1、SBT 或 DispatchRays | `01_platform_device.cpp`、`ADVANCED_RENDER_INTERFACES.md` 1.4 |
| `0x142478621`、`0x1424745E4` | Device alias / `ReportGpuHaltX` | reconstructed | 初始化证明 `qword_148759D00=qword_1475DF048`，故 Device132 的 `(device,3)` 确实属于同一设备；返回类型不能由忽略 EAX 推断 | `UNRESOLVED.md` |
| `0x141F77AD0`、`0x140F9F650`、`0x14247FD30` | Shared ShaderProgram layout | reconstructed | 352 字节共享对象；`+0x118` 为共用 root，`+0x120` 在 compute 为 PSO、DXR 为 StateObject；compute root 不是序列化 blob | `07_pipeline_programs.cpp`、`09_low_level_calls.cpp` |
| `0x140FE4E50`、`0x142490110` | `BuildBatchedAccelerationStructures` | reconstructed | 80 字节批量描述、update/scratch/compaction、两条路径和 barrier 边界已恢复 | `09_low_level_calls.cpp`、`ADVANCED_RENDER_INTERFACES.md` |
| `0x140BD96A0` | `CopyCompactedAccelerationStructures` | reconstructed | 目的/源地址数组，COMPACT copy | `09_low_level_calls.cpp` |
| `0x140EA6C60` | `RehydrateAccelerationStructures` | reconstructed | 每批最多 16 个 desc，标准 56 字节序列化头、Source/originalInstanceData 和最终 UAV barrier 已恢复 | `09_low_level_calls.cpp` |
| `0x140075480` | `QueryPackedAccelerationStructurePrebuildInfo` | reconstructed | TOP_LEVEL instance / BOTTOM_LEVEL geometry、transform 复制、alias、compaction 和 update scratch | `09_low_level_calls.cpp` |
| `0x144EE2A00` | `QueryBottomLevelPrebuildInfoPreservingMortonMode` | reconstructed | 不改变 Morton；48→56 字节 triangles/AABBs 转换与 result/scratch 输出 | `09_low_level_calls.cpp` |
| `0x1456AC100` | `QueryBottomLevelPrebuildInfoWith64BitMorton` | reconstructed | 始终启用 64 位 Morton、查询后恢复 false | `09_low_level_calls.cpp` |
| `0x140F2C470` | `MapPhysicalPagesAndQueueMappings` | reconstructed | 六种 packed page commands、页索引/free-list、scatter/batch、排序和 VA 去重 | `09_low_level_calls.cpp` |
| `0x142215FD0` | `MigrateToolsMemoryWhenRequired` | reconstructed | KernelPool 条件、第二份统计的 ToolsLimit-ToolsUsed、2MiB/16MiB 迁移阈值 | `09_low_level_calls.cpp` |

## 已复核的 XS 调用锚点

| 调用地址 | 对象槽位 | 当前名称 | 证据 |
|---|---:|---|---|
| `0x1424785BE` | 导出函数 | `D3D12XboxCreateDevice(..., IID_ID3D12Device11, ...)` | B+H |
| `0x14247878F` | Device `+0x40` / 8 | `CreateCommandQueue` | B+H |
| `0x14247881B` 等 | Device `+0x180` / 48 | `CreateCommandQueueX` | B+H；四个 Type=compute |
| `0x142478B32` | Device `+0x48` / 9 | `CreateCommandAllocator` | B+H |
| `0x142478B79` | Device `+0x248` / 73 | `CreateCommandListX` | B+H |
| `0x142478B9E` | GCL `+0x420` / 132 | `SyncBottomOfPipeX` | B+H；四显式参数 |
| `0x142478BBF` | GCL `+0x418` / 131 | `SyncTopOfPipeX` | B+H；四显式参数 |
| `0x142478BD2` | GCL `+0x48` / 9 | `Close` | B+H |
| `0x142474CD6` | GCL `+0x458` / 139 | `ResetX` | B+O+H；本 EXE 直接调用 |
| `0x1424821BD` | Queue `+0x118` / 35 | `ExecuteCommandLists2X` | B+H；携带 token 和 flags=3 |
| `0x1424824C1` | Queue `+0x100` / 32 | `PresentX` | B+H；plane 步长 `0x48` |
| `0x142482608` | Queue `+0x80` / 16 | `Signal` | B+H；批次尾部 fence |
| `0x142482450` | Device `+0x280` / 80 | `WaitFrameEventX` | B+H+D；flags=2 为 `PROCESS_TOKEN_ON_TIMEOUT` |
| `0x14193E1B4` | Device `+0x3F8` / 127 | `CreateCommittedResource3` | B+PC；十显式参数 |
| `0x14193E000/0x14193E2DD` | Device `+0x428` / 133 | `CreatePlacedResourceX1` | B+O；六显式参数 |
| `0x141E6C2C3` | Device `+0x250` / 74 | `RegisterPagePoolX` | B+H；VA、页数、句柄 |
| `0x141CF32FD` | Device `+0x70` / 14 | `CreateDescriptorHeap` | B+H |
| `0x140164EFB` | Device `+0x90` / 18 | `CreateShaderResourceView` | B+H |
| `0x141647610` | Device `+0x98` / 19 | `CreateUnorderedAccessView` | B+H |
| `0x140CC29E4` | Device `+0xA0` / 20 | `CreateRenderTargetView` | B+H；反汇编补回 R9 handle |
| `0x1414A69E9` | Device `+0xA8` / 21 | `CreateDepthStencilView` | B+H |
| `0x1404F46B5` | GCL `+0x78` / 15 | `CopyBufferRegion` | B+H |
| `0x1404F4574` 等 | GCL `+0x2A8` / 85 | `CopyTextureRegionX` | B+H；260405 平移后槽位 |
| `0x1404F4B4C` | GCL `+0x2B0` / 86 | `CopyResourceX` | B+H；260405 平移后槽位 |
| `0x14003210D` | GCL `+0x490` / 146 | `Barrier` | B+PC；pending barrier 数量和数组 |
| `0x142481B33` | Device `+0x138` / 39 | `CreateQueryHeap` | B+H；IID_ID3D12QueryHeap |
| `0x14247953C..0x14247962A` | Device `+0x148` / 41 | `CreateCommandSignature` | B+H；五次创建 |
| `0x14248CD10`、`0x14009444D`、`0x144E33F91` 等 | GCL `+0x1E8` / 61 | `ExecuteIndirect` | B+H；9 个包装，全部 max=1、count buffer 为空，含独立/共享参数 |
| `0x1424813F9` | Device `+0x328` / 101 | `CreateStateObject` | B+H；IID_ID3D12StateObject |
| `0x140FE4FEC` 等 | XboxBvhBuilder 导出 | `BuildRaytracingAccelerationStructureBatched2X` | B+H+DLL；DLL `0x1800216A0` 验证实现 |
| `0x140EA6D5A` 等 | XboxBvhBuilder 导出 | `RehydrateRaytracingAccelerationStructureBatched2X` | B+H+DLL；DLL `0x1800219D0` |
| `0x140F2CC7F` | Queue `+0xF8` / 31 | `CopyPageMappingsBatchX` | B+H；与 XMemMapPhysicalPagesScatter 组合 |

## 水面模拟与 CPU 采样

| 地址 | 当前语义命名 | 状态 | 主要证据 | 对应输出 |
|---|---|---|---|---|
| `0x142625EE0` | `WaterCompressedSimulation_RecordPass` | reconstructed（GPU 端） | compressed-simulation 资源绑定与 Compute 调度 | `10_water_simulation.cpp`、`WATER_SIMULATION_READBACK.md` |
| `0x140BA1240` | `WaterSimulation_RecordBatch` | reconstructed | 单组 Compute 调度；`+0x30` 仅置有效，不是 fence | 同上 |
| `0x140294DB0` | `OceanWaveSim_RecordCompute` | GPU-only 已确认 | 多资源 Compute；未找到 CPU alias 消费者 | `10_water_simulation.cpp`、`WATER_SIMULATION_READBACK.md` |
| `0x140047640` | `WaterSimulation_SampleCpu` | reconstructed（CPU 端） | 三模式批量输出、逐查询变体、uint8/uint16 权重插值、有效标志和 Inf/NaN 清理 | `10_water_simulation.cpp` |
| `0x14291347B`、`0x144FD2940` | `WaterSimulation_GetCpuAlias` | reconstructed | 资源地址转换为 CPU alias | 同上 |
| `0x1453F7B90`、`0x141FB4F70` | `WaterReadbackSlot_Next/Advance` | reconstructed | 三槽环；CPU 读取 `(current+1)%3` | 同上 |

GPU pass 与 CPU 查询属于同一水面子系统，但逐资源的“本次 dispatch 原生资源 == 本次查询 alias”仍需动态地址对照；不能仅凭记录布局相似宣称逐资源闭环。

## 数值常量来源

| 数值 | 语义化名称 | 来源 |
|---:|---|---|
| 96 | `kSubmissionBatchCount` | 生产者和消费者均以 96 回绕 |
| 80 | `kSubmissionRecordSize` | 消费者记录指针步长 |
| 72 | `kSubmissionBatchHeaderSize` | 每批 9 个 64 位字段 |
| 256 | `kMaxBatchedCommandLists` | 三个 256 项栈数组分别保存 GCL、完成回调、上下文 |
| 24 | `kInternalPresentPlaneSize` | 内联 plane 输入记录步长，包含 resource/colorSpace/width/height，不是 XS plane 大小 |
| 2 | 最大内联 Present plane 数 | 80 字节记录在 +0x08..+0x37 内联两个 24 字节 plane |
| `0x48` | `sizeof(D3D12XBOX_PRESENT_PLANE_PARAMETERS)` | 消费者输出 plane 步长；alignof=8 |
| 11 | `kPooledCommandListSlotCount` | 初始化循环槽位 0..10；只有 0..4 在此函数实际创建对象 |
| 4 | `kComputeQueueCount` | 四次 CreateCommandQueueX，描述均 Type=2 |
| 2 | `kFrameObjectSlotCount` | `Platform_AdvanceFrameIndex` 的 `index & 1` |
| 4096 | `kDeferredCallbackSlotCount` | 回调索引 `(index + 1) & 0xFFF` |
| 25 | `GfxFrameInterface` 项数 | `off_1461ECC90` 的 25 个 64 位函数指针 |
| 48 | deferred-resource 注册数 | `Gfx_FlushDeferredResources` 中 48 次构造 + 48 次 slot13 调用 |
| 64 | deferred 注册表容量 | `Gfx_InterfaceInit` 对 qword_1475DDC18 初始化 |
| 72 | `PresentationViewState` 大小 | 资源销毁/重建函数按 72 字节索引 view |
| 688 | graphics PSO stream 字节数 | `0x14086F090` 的 memset 大小和 `CompileGraphicsProgram` 的 stream.SizeInBytes；15 个子对象完整覆盖 |
| 15 | graphics PSO 子对象数 | root、5 个 shader、blend、sample-mask、rasterizer、DS1、sample-desc、IA、topology、RT formats、DS format |
| 204800 | PSO 编译 scratch 字节数 | graphics/compute 的 TLS 临时区分配 |
| 2 | `D3D12XBOX_WAIT_FRAME_EVENT_FLAG_PROCESS_TOKEN_ON_TIMEOUT` | GDK flags 文档；timeout 仍生成 token |
| 16 | `sizeof(D3D12_BARRIER_GROUP)` | 增强 barrier 组头和当前 EXE 步长 |
| 40 | `sizeof(D3D12_BUFFER_BARRIER)` | `0x1400095E0` 的 buffer barrier 步长 |
| 64 | `sizeof(D3D12_TEXTURE_BARRIER)` | `0x1400097F0/0x1400099F0` 的 texture barrier 步长 |
| 257 | `kBarrierGroupCapacity` | `0x101`，达到上限先 flush |
| 1025 | `kBarrierResourceCapacity` | `0x401`，每类资源 barrier 达到上限先 flush |

注意：`flags=3` 已在头文件中解释为 `NO_SWITCH_BUFFER | CHAIN_SEGMENTS`；`CreateDeviceFlags=0x20` 在本地旧头文件中没有对应命名，伪代码保留 `kCreateDeviceFlag_Unresolved_0x20`，不能伪造枚举名。

本轮独立复核还修正两点：direct/compute 的 top sync 分别为 `0xC3C0/0xC380`，compute 不含 PFP_SYNC_ME，两者都不含 MCACHE 位；内存池选择器的 9 是每个 memoryType 的变体槽数，不是全局内存类型总数。

## 保留的未决项

- Device 126、Device 130/131 的顺序继续沿用主 XS 报告的证据等级，不在渲染伪代码中调用。
- packed synchronization state 的 bit 31 保留；其余位域和计数信号量行为已恢复，不展开无关的等待节点池。
- `PresentationViewState` 保留未证明的锁字段；链表为 `+0x28` 哨兵、`+0x30` 首节点、`+0x40` 计数。
- 30 个 deferred 回调只有 `RenderPassXX` 行为名，具体原始 pass 名见 `UNRESOLVED.md`，不根据常见渲染流程猜名。
- 私有硬件 CBV flags 的逐位含义和全部格式枚举继续保持原证据等级；CBV 字节公式、内存类型和派生 PSO 类别已闭环。完整替换阻塞矩阵见 `BACKEND_REPLACEMENT_CLOSURE.md`。

## 老引擎交叉复核

`LEGACY_ENGINE_COMPARISON.md` 记录第二次完整对照，主要老源码锚点包括：

- `device_d3d11.cpp:2661-2829`：BeginFrame 等待旧 fence、清状态。
- `device_d3d11.cpp:2840-3098`：EndFrame、帧 fence、Resolve 和 Present。
- `gfxcontext_durango.cpp:979-1081`：Close、kick、cache 和 ExecuteCommandList。
- `device_d3d11.cpp:6547-6594`：Draw 前状态刷新。
- `rendertarget_durango.cpp:798-979`：XG 布局、地址分配和 placement texture。
- `texture_durango.cpp:561-615`：线性 mip 数据到 tiled backing 的 CopyIntoSubresource。
- `device_d3d11.cpp:3102-3205`：fence 分配、GPU 写入和 CPU 等待。

该对照用于验证语义延续，不用于提高任何新 XS 槽位的证据等级，也不作为代码提取来源。当前实施方案见 `CLEAN_RENDER_BACKEND_PLAN.md`：直接重新实现干净的桌面 D3D12 引擎，不移植 GTA5 代码。
