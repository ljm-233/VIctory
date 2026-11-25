# C++ 类与结构索引

本索引帮助按对象职责阅读伪代码。带“逻辑类”的条目是对全局/TLS 状态的 C++ 组织，不表示原二进制存在同名类；带“ABI”的条目则有实际字段偏移或步长证据。

## 平台设备与队列

### `PlatformRenderDevice` — 逻辑类
文件：`pseudocode/01_platform_device.cpp`

拥有 Device11、一个 direct queue、四个 compute queue、队列上下文、11 个列表槽和 direct/compute 池。初始化只创建槽 0..4，槽 5..10 保留。设备指针被多个子系统共享，不是多个设备实例。

关键函数：`CreateWindowAndDevice`、`CreateQueuesAndContexts`、`CreateInitialCommandLists`。

### `QueueContext` / `CommandListSlotEntry`
- `QueueContext`：queue、当前 allocator 和上下文 ID。
- `CommandListSlotEntry`：allocator 与 GCL 的配对。

### `DirectCommandListPool` — ABI 辅助模型
16 字节节点、原子 packed free-list head。获取节点后调用 `ResetX(list, allocator, nullptr, NONE)`。

### `ComputeCommandListPool` — ABI 辅助模型
8 字节指针槽、锁和 round-robin 索引。与 direct free-list 不是同一算法。

### `RenderThreadCommandContext` — 逻辑类
负责线程懒初始化、queue 绑定、列表选择、当前池节点和命令段状态。获取函数权威地址为 `0x140188430`。

## 帧提交与显示

### `SubmissionBatch` — ABI，72 字节
文件：`pseudocode/02_frame_submission.cpp`

字段：queue、单发列表槽、记录指针、容量、起始索引、数量、操作等待状态、完成 fence、CPU 完成值地址和 fence value。记录存储不由批头内联拥有。

### `SubmissionRecordStorage` — ABI，80 字节
四种记录：

- `SubmitRecord`：GCL、完成回调对象、上下文和控制标志。
- `CallbackRecord`：CPU 回调与上下文。
- `PresentRecord`：内部 plane、显示时序、frame serial 和完成标志。
- `FrameTokenRecord`：显式 token 或待获取 token，以及 frame serial。

### `InternalPresentPlane` — ABI，24 字节
资源指针、色彩空间、源宽度和源高度。`PresentRecord` 内联最多两项，不使用外部 plane 指针；它不是 XS plane。

### `D3D12XBOX_PRESENT_PLANE_PARAMETERS` — XS ABI，`0x48` 字节，align 8
由消费者根据内部 plane 重新构造，包含 token、资源数组、矩形、色彩空间、scale/filter、扩展描述和 flags。

### `FrameSubmissionQueue` — 逻辑类
96 批环形队列。消费者聚合最多 256 个 GCL，批量调用 `ExecuteCommandLists2X`，按记录边界处理 token 和 Present，最后 Signal fence。提交回调不等于 GPU 完成。`flushAndExecutePooledList` 是 Submit/Callback 共用的额外冲刷与池化列表提交标志。

### `PackedSynchronizationState` — ABI，8 字节
计数信号量：bits 0..6 是等待节点 token（127 为空），bits 7..30 是自旋预算，高 32 位是可用计数；bit 31 保留。

### `FrameLifecycleCoordinator` — 逻辑类
文件：`pseudocode/06_frame_interface.cpp`

维护 end-request、pending、flush-in-progress、scope depth、显示资源 ready 状态和 deferred registry。`EndFrameAndFlush` 负责 deferred 执行及显示资源重建。

### `GfxFrameInterface` — ABI，25 项
只包含尺寸/矩形、deferred、帧状态与 scope。它不是完整渲染设备虚表。

### `DeferredFrameCallbackRegistry`
64 个 32 字节闭包。`callbackTarget` 参与去重，`callback` 是统一 trampoline。执行后保留注册项，只有 pending 被清除。

## 资源、地址与描述符

### `GraphicsVirtualMemory` — 逻辑类
文件：`pseudocode/03_resource_memory.cpp`

负责一次性 `D3D12ConfigureVirtualMemoryX`、两段 VA 保留和 placement 地址转换。配置必须先于图形分配和 CreateDevice。

### `PhysicalPagePool` — 逻辑类
64 KiB 页、D3D page-pool 句柄、页映射 queue、21 位页索引、每 qword 三项的物理页表和 free stack，以及每页状态字节。

### `MemoryPoolSet`
以 `9 * memoryType + 1 + subtype` 选池。9 是每类池变体数，不是全局类型总数。地址分配扩展路径固定 variant 1、主路径固定 variant 3，释放使用同一槽位；memoryType 只影响 128 字节对齐。初始化 0/2/4/5 为专用实现，1/3/6/7/8 为可替换通用实现。

### `RageResource` — 部分 ABI
保存资源 flags、memoryType/subtype、GPU 地址、`ID3D12Resource*`、格式、创建标志及关联状态。支持 committed、forced placed、backing-only 和普通 placed 路径。

### `RageFormatDesc` — 12 字节完整布局
`+0/+2/+4` 为 width/height/depthOrArraySize，`+6` 为 RAGE dimension，`+7` 直接是 DXGI_FORMAT，`+8` 为 swizzle，`+9` 为经 17 项 DWORD 表映射的 sampleMode，`+0xA` 为 mipCount，`+0xB` 为 layoutFlags。后续 `+0x44/+0x45` 是独立 creation/allocation flags，不属于这 12 字节。

### `RageBuffer`
共享资源前缀，`+0x38` 在 CPU 地址与 CBV 描述符存储之间复用。并非所有 buffer 都必须创建 `ID3D12Resource`。

### `DescriptorHeap`
字段包含 XS heap、CPU begin/end、increment、GPU delta 和 external-CPU-base 标志。非 shader-visible 使用保留哨兵，不能把 GPU delta 当普通地址；`+0x40` 不表示已证明的堆所有权。

### `DescriptorRangeAllocator` / `DescriptorFreeList`
前者是原子对齐 bump allocator，后者是 packed-head free-list，返回节点地址作为 CPU descriptor handle。

### `DescriptorFactory` / `HardwareCbvDescriptor`
统一 SRV/UAV/CBV 创建。CBV 有原生 `CreateConstantBufferView` 和直接写 16 字节硬件描述字两种模式。软件布局为 tagged address、16 字节单元计数和 `0x2403C` flags，已可逐字节替换。

### `RenderTargetView` / `DepthStencilView`
`+0x08` backing resource，`+0x20` descriptor storage。RTV 的 R9 handle 已由原始汇编补回，不能因 Hex-Rays 少显示一个参数而改成三参调用。

### `UploadStaging`
包含 CPU 地址、资源内字节偏移、资源及是否来自线程 arena。小上传使用共享上传资源，大上传使用独立临时资源并延迟释放；`CopyBufferRegion::SrcOffset` 使用资源内偏移，不能使用 GPU VA。

## 命令与状态

### `CommandListEncodingState` — XS 实现对象部分布局
文件：`pseudocode/05_command_recording.cpp`

- `+0x18` write cursor。
- `+0x28` soft packet limit。
- `+0x30` allocation end。
- `+0x140..0x143` 编码状态标志。
- `+0x144` root opcode table。

标题会直接写这些字段，因此 Hook GCL 函数表无法覆盖全部快路径。

### `RenderThreadContext` — 逻辑类
维护 requested/dirty/encoded 状态、PSO、IA、OM、viewport/scissor、root binding 和 pending barriers。`DrawIndexed` 先刷新状态，再写 24 字节包或调用 GCL slot 13。

### `PipelineProgramKey` / `PipelineProgram`
key 描述 shader、IA、root、rasterizer、depth/blend、topology、sample 等身份；program 持有 native PSO 和编译状态。完整缓存定义在 `07_pipeline_programs.cpp`，Draw 只消费 ready 对象。

### `PendingBarrierSet`
累计增强资源转换，在实际 Draw/Copy 等边界调用 GCL146 `Barrier` 后清空。`count` 是 16 字节 `D3D12_BARRIER_GROUP` 的组数，不是转换总数；组容量 257。buffer barrier 为 40 字节，texture barrier 为 64 字节，各自容量 1025。达到容量时先提交，再继续分组。

### `RootBindingCache`
四组 descriptor table、root SRV/UAV 地址和 root constants。区分 CPU staging 地址、GPU descriptor handle 和已经编码的 handle。

### `ShaderBindingLayout` — 部分 ABI
`+8` descriptor count，`+11/+13/+15` SRV/UAV/CBV 数量，`+80..91` root 映射；binding ID 从 `+24` 起。

### `ShaderBindingId`
- `bits 0..10`：资源索引。
- `bits 12..13`：默认描述符类别。
- `bit 15`：选择第 4 类默认描述符。
- `DestinationByteOffset=(id>>11)&0x7FE0`。

## PSO 缓存

### `GraphicsProgramCache`
文件：`pseudocode/07_pipeline_programs.cpp`

无锁发布快照和加锁写入表。key 未命中才创建逻辑 program，再决定同步编译或投递编译任务。`GraphicsPipelineStateStream` 完整声明 688 字节、15 个子对象及所有偏移，输入元素描述数组位于独立 scratch 区。

### `DerivedGraphicsPsoCache`
按 hash/base PSO 查缓存；不匹配时按 rasterizer、depth/stencil、可选 depth options、blend 的顺序生成 332 字节派生描述并调用 Device55。

### `SerializedComputePsoCache`
按 shader hash 找序列化项，调用 Device67；未命中由 Device62 创建。

### `PipelineCompileScratchScope`
200 KiB 线程临时区的作用域管理。graphics 使用其中 688 字节作为 stream 描述，返回后回收，不允许异步继续使用该栈区。

### `ComputeProgram` / `ShaderBlobHeader`
Compute 的 `a1[35]` 是 root signature，`a1[36]` 是 native PSO；序列化缓存是独立快路径，不能把 root 字段命名为 serialized。Shader blob 使用 12 字节 `0xDCE7` 头，codec 3 为 Stored、13 为 raw DEFLATE，`+4` 是含头总大小、`+8` 是输出大小，输出由编译 scratch/临时池管理。

## 复制与视图辅助类型

`GpuCopyLocation` 区分线性 CPU 数据和 D3D resource，包含 offset/size/subresource/box。`RecordResourceCopy` 根据源/目的类型选择 `CopyBufferRegion`、`CopyResourceX` 或 `CopyTextureRegionX`。

## 其他底层对象

### `IndirectCommandSignatures`
5 个签名分别用于 Draw、DrawIndexed、DispatchL2、Dispatch 和 DispatchMesh。9 个 ExecuteIndirect 包装使用 GCL slot 61，覆盖独立/共享参数缓冲；全部 `maxCommandCount=1` 且 count buffer 为空，不与 DrawIndexed 共用内联包。标准 0x408 与私有 0x320 均未证实 direct mesh 调用，间接 XYZ 记录为 12 字节，不另造 direct wrapper。

### `RaytracingPipelineFields` / `HitGroupShaderNames`
这是 352 字节共享 ShaderProgram 的 DXR 变体视图，保存 global/两份 local root、StateObject、Properties 和 identifier 指针。`+0x118` global root 与 compute 共用；`+0x120` 在 compute 路径是 PSO，在 DXR 路径才是 StateObject。创建时生成 N+9 个子对象；Properties 与 StateObject 是不同函数表，identifier 指针依赖 StateObject 存活。

### `WaterSimulationRecord` / `DesktopWaterReadbackRing`
水面物理采样记录包含 CPU alias、GPU 资源视图、有效标志和行跨度。当前一组 CPU 资源为 64×64 R32G32F；查询使用三槽旧结果，`+0x30` 不是 fence。桌面实现使用独立 readback heap、每槽 fence 和非阻塞完成查询，详见 `WATER_SIMULATION_READBACK.md` 与 `pseudocode/10_water_simulation.cpp`。

### `BvhBuildInfo` 与 BVH 调用族
build-info 前 `0x48` 字节包含 prebuild、compacted-size 复制信息和 Inputs。批量描述为 80 字节；update 选择 update scratch 并设置 PERFORM_UPDATE，非 update 且允许 compaction 时生成 compacted-size 查询。两条路径分别处理 scratch 容量/compaction 和 64 位 Morton codes。另有 COMPACT copy、重水合和 PrebuildInfo。详见 `pseudocode/09_low_level_calls.cpp`、`ADVANCED_RENDER_INTERFACES.md` 和 `LOW_LEVEL_COVERAGE_AUDIT.md`。
