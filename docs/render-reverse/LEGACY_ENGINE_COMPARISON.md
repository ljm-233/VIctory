# 老 RAGE 与 Scarlett 渲染接口层复核

## 0. 对照范围与结论

本次重新读取老引擎源码，而不是仅沿用先前的对应表：

- 老引擎：`C:\Users\Administrator\Desktop\phy\AndroidGTAV`
- 新引擎权威：`game_scarlett_master.exe.i64`
- 新模型：`pseudocode/01..08`、`EVIDENCE_INDEX.md` 和 `DOCUMENT_CROSSCHECK.md`

**总体判断：两代 RAGE 的高层设计具有明确连续性，新渲染流程在老接口语义上说得通。主要变化不是“换了一组随意 API”，而是把 D3D11 时代的对象、隐式状态和驱动同步，拆成 D3D12/XS 的显式 PSO、descriptor、地址、barrier、queue、token 和 fence。**

老源码只用于验证渲染语义，不能用于决定 Scarlett 的 XS 数值槽位或私有结构偏移。**当前实施方案已改为直接重新实现干净的桌面 D3D12 引擎，不提取或移植本文中的 GTA5 代码；本文仅保留为语义交叉分析。**

## 1. 渲染接口不是一个单一后端虚表

老 `grcDevice` 在 `device.h:603-647` 同时承担初始化、帧、shader、状态、绘制和同步。`device.h:431-438` 的 `grcEntry static`、`grcPure` 为空，说明它不是一张统一的可替换后端虚表。

新引擎同样没有单一总虚表：

- `off_1461ECC90` 的 25 项只负责尺寸、deferred 和帧控制。
- Device11/Queue/GCL 是 XS 对象函数表。
- RAGE 线程上下文、提交环和资源池是独立实现。
- 快路径直接写 GCL 内部命令段，绕过方法表。

因此“把老 `grcDevice` 替换为另一后端”与“只 Hook 新 Device11/GCL”都不是完整替换方案。这与前面得到的 Hook 结论一致。

## 2. 初始化与帧边界

### 老引擎

`device_d3d11.cpp:2661-2829` 的 `grcDevice::BeginFrame`：

1. 等待两帧前的 latency fence（`:2667-2673`）。
2. 清理 hazard 和缓存状态。
3. `grcStateBlock::MakeDirty/Default`（`:2772-2777`）。
4. 获取 back buffer，初始化 effect 和默认状态。

`device_d3d11.cpp:2840-3098` 的 `EndFrame`：

1. 结束 effect、清理流输入和 MSAA resolve。
2. 写本帧 latency fence（`:2944-2951`）。
3. Durango 调用 `grcGfxContext::endFrame`（`:2953-2955`）。
4. 执行 `DXGIXPresentArray` 或 swapchain Present（`:2957-3004`）。
5. 释放上下文（`:3069-3070`）。

### 新引擎

- `GameFrame_Update` 负责渲染结果和 pending 检查。
- `Platform_FrameStep` 负责视图、Present 入队和帧索引推进。
- `Gfx_EndFrameAndFlush` 负责 deferred 闭包、显示资源销毁/重建和帧状态。
- 提交消费者在 Execute/Present 后 Signal fence。

### 判断

职责延续明确，但 **`grcDevice::EndFrame` 不等于 `Gfx_EndFrameAndFlush`**：

- 老 EndFrame 自己结束上下文并 Present。
- 新 EndFrameAndFlush 主要执行 deferred 和资源生命周期操作，Present 由 `Platform_FrameStep` 和异步消费者完成。

新流程把“逻辑帧结束”“资源重建”“命令提交”“显示呈现”拆得更细。

## 3. 线程命令上下文与提交粒度

### 老引擎

`gfxcontext_durango.h:77-158` 的 `grcGfxContext` 与 `ID3D11DeviceContextX` 组成逻辑上下文，包含：

- 当前 allocation scope。
- data segment / command-buffer segment。
- deferred 标志和控制对象。
- 当前 D3D command list。

`gfxcontext_durango.cpp:979-999` 在 close 时释放段并 `FinishCommandList`。`grcGfxContextCommandList::kick`（`:1001-1081`）执行：

```text
prepareSubmit
→ 必要时 flushGpuCaches
→ ExecuteCommandList
→ partial submit 时额外 wait/invalidate
→ Release
→ Flush
```

`gfxcontextcontrol.h:38-249` 还用 packed atomic state 管理 pending、partial submit、线程暂停和全局段配额。

### 新引擎

- `RenderThreadCommandContext` 懒获取 GCL 和池节点。
- direct 池是 free-list，compute 池是加锁 round-robin。
- `CommandListEncodingState` 支持直接写 packet 和慢速 XS 调用。
- 96 批 `FrameSubmissionQueue` 把提交、回调、token、Present 编成 80 字节记录。
- 消费者聚合最多 256 个 GCL，再调用 `ExecuteCommandLists2X`。

### 判断

两者都在做“多线程分段录制 + 配额 + 延迟回收 + 必要时部分提交”。区别是：

- 老 kick 是 `ExecuteCommandList + Flush`，带 immediate context 和 D3D11 deferred context。
- 新消费者是批量 `ExecuteCommandLists2X + frame token`，XS GCL 和 allocator 池显式复用。
- 新 `CHAIN_SEGMENTS` 保留了段链式提交思想，但不是老 `grcGfxContextCommandList::kick` 的逐句翻译。

老 partial submit 的额外 wait/cache invalidate 是明确的性能代价（`gfxcontext_durango.cpp:1035-1053`）；新引擎是否更高效需要运行时数据，不能只凭结构断言。

## 4. Dirty-state 与 Draw 前刷新

### 老引擎

每次 Draw 前：

```cpp
grcStateBlock::Flush();
SetUpPriorToDraw(dm);
```

见 `device_d3d11.cpp:6601-6605`、`:6652-6657`、`:6709-6714`。

`SetUpPriorToDraw`（`:6547-6594`）依次处理：

- input layout。
- vertex buffer（非 Durango 路径）。
- primitive topology。
- 当前 state。
- program resources。
- clip planes。

`stateblock.cpp:2410-2418` 只刷新 dirty 的 depth/rasterizer/blend 状态；`:2492-2499` 的 `MakeDirty` 使缓存失效。`effect_durango.cpp:171-195` 还在 program bind 时更新 dirty constant buffer，并调用 `SetFastResources`。

### 新引擎

`RenderThreadContext::DrawIndexed` 和 `FlushFullState`：

- 合并 requested/deferred/encoded dirty 状态。
- 复杂状态走 full flush，root 子集走轻量 flush。
- 刷新 topology、PSO、IA、RT/DS、viewport/scissor、root binding 和 pending barriers。
- 有空间写 24 字节 DrawIndexed 包，否则调用 GCL slot 13。

### 判断

这是最直接的设计延续：**Draw 前集中处理脏状态，而非每个 setter 无缓存地下发。**

但 `grcStateBlock::Flush` 与 `FlushFullState` 不是同一函数：

- 老对象主要是独立 D3D11 state 和 program bindings。
- 新对象增加了 PSO 身份、descriptor heap、root signature 和显式 resource barrier。
- 新 dirty bit 与老 `BLEND_STATE_DIRTY` 等位没有一一数值映射。

## 5. Shader、状态对象与 PSO

### 老引擎

- D3D11 shader/program 独立创建和绑定。
- rasterizer/depth-stencil/blend/sampler 是独立状态对象。
- `grcProgram::BindCommon` 更新常量并绑定 shader resources。
- `SetProgramResources` 在 Draw 前完成 program 绑定（`device_d3d11.cpp:6513-6544`）。

### 新引擎

`GraphicsProgramCache` 将 shader、IA、root、rasterizer、depth/blend、topology、sample 等组成 `GraphicsProgramKey`：

```text
发布表/写入表查找
→ 创建逻辑 program
→ derived PSO cache 查找
→ 688 字节 graphics stream
→ CreatePipelineState / CreateDerivedGraphicsPipelineStateX
```

compute 另有序列化缓存和 `CreateComputePipelineStateX` fallback。

### 判断

老 `grcProgram` 到新 `PipelineProgram` 的演进合理：高频状态从多个独立对象合并为 PSO，减少逐状态驱动工作。新引擎仍保留 key/cache/异步创建，说明它并没有把老的高层“按效果组织状态”全部丢弃。

## 6. Placement 资源与统一内存

### 老引擎

`buffer_durango.cpp:333-355`：

```text
XG/desc
→ m_pGraphicsMemory
→ CreatePlacementBuffer(existingAddress)
```

`rendertarget_durango.cpp:798-979`：

```text
格式和 bind flags
→ XGComputeOptimalTileMode
→ XGCreateTexture2DComputer + GetResourceLayout
→ 分配物理/虚拟地址
→ CreatePlacementTexture2D(existingAddress)
→ 创建 SRV/RTV/DSV/UAV
```

`texture_durango.cpp:622-654` 同样区分 D3D resource 和其 backing address。

### 新引擎

`CreateRageResource`：

- committed 路径使用 `CreateCommittedResource3`。
- placed 路径按 memoryType/subtype 获取地址，使用 `CreatePlacedResourceX1`。
- 另有 backing-only 和已有地址 forced-placed 路径。
- 物理页、GPU VA、resource 和 descriptor 分开管理。

### 判断

`CreatePlacementBuffer/Texture(existingAddress)` 到 `CreatePlacedResourceX1(GpuVa,...)` 是非常清晰的演进。两代都把“内存所有权”与“D3D 资源对象”分离。

差异是新引擎把该模型扩展为更完整的地址池、页池、page mapping 和延迟回收；老 ESRAM/物理页专用策略不能直接搬到 Scarlett。

## 7. XG 纹理初始化与上传

### 老引擎

`texture_durango.cpp:561-615` 的 `CopyOverTexels`：

1. 创建 XG address computer。
2. 按 mip 组织连续线性 mip-slice。
3. 按行复制到连续临时存储。
4. `CopyIntoSubresource` 写入 tiled graphics memory。
5. 释放 computer。

`CreateGivenDimensions`（`:1249-1329`）先计算 tile mode、mip offsets，再组织初始 mip 数据。

### 新引擎

`UploadLinearTextureToTiledMemory`：

- 使用 `XGCreateTextureComputer`。
- 按 plane/mip/slice 计算 row pitch、slice pitch 和线性偏移。
- 调用 `CopyIntoSubresource`。
- 另有 `RecordLinearCpuToTexture`：线性 CPU 数据先复制到 256 对齐 staging，再 `CopyTextureRegionX`。

### 判断

XG 地址计算和 `CopyIntoSubresource` 的用途完全一致：把线性输入变成硬件 tiled 布局，不是普通 memcpy。

需要区分两条路径：

- XG 写入 backing 是创建/加载时的 CPU 侧 tiling。
- `CopyTextureRegionX` 是录制到 GCL 的 GPU copy。

把两者混成一种上传会错误估计带宽和同步要求。

## 8. View、SRV/UAV 与 Descriptor

### 老引擎

`texture_durango.cpp:649-654` 创建 `ID3D11ShaderResourceView` COM 对象；RT/DS 也有独立 D3D view 对象。绑定通过 `SetFastResources` 或 `VSSet/PSSetShaderResources`。

### 新引擎

- `ID3D12Resource` 与 SRV/UAV/RTV/DSV descriptor 分开。
- descriptor heap 保存 CPU begin/end、increment 和 GPU delta。
- free-list/bump allocator 管理 handle。
- root table 需要 CPU staging → GPU descriptor handle。
- 空绑定回退到默认描述符。
- CBV 还存在直接写两个硬件描述字的模式。

### 判断

老 view 的“资源解释方式”延续到新 descriptor，但对象模型不同：

```text
老：ID3D11Resource + COM view 对象
新：ID3D12Resource + heap 中的 descriptor handle + root binding
```

不能把老 `ID3D11ShaderResourceView*` 简单替换成新 GPU descriptor handle，两者生命周期和地址空间不同。

## 9. Fence、cache 与显式同步

### 老引擎

`device_d3d11.cpp:3102-3205`：

- `AllocFence` 创建 CPU 可见状态。
- `GpuMarkFenceDone` 使用 `WriteValueBottomOfPipe`。
- `IsFencePending` 检查状态，必要时触发 partial submit。
- `CpuWaitOnFence` 轮询等待。

`gfxcontext_durango.cpp:470-481` 的 `flushGpuCaches` 先 `InsertWaitUntilIdle`，再由 PFP invalidate L1/L2/KCACHE。`kick` 在 partial submit 时还进行额外 wait 和 cache invalidate。

### 新引擎

- GCL `SyncTop/BottomOfPipeX` 处理 pipe stage 和 cache。
- `PendingBarrierSet` 在 Draw/Copy 前调用 GCL146 `Barrier`。
- Queue `Signal` 写完成 fence。
- `WaitFrameEventX(PROCESS_TOKEN_ON_TIMEOUT)` 生成帧 token，但 token 不是 fence。
- 完成状态驱动 allocator、命令段和资源的跨帧回收。

### 判断

老 `WriteValueBottomOfPipe` 与新 Queue Signal 都用于完成通知；老 `grcFenceHandle` 对应新 fence，**不对应 frame pipeline token**。

老 D3D11 hazard tracking 与新显式 barrier 也不能混为一谈：新引擎必须正确维护资源 layout、cache 和 queue 顺序，不能只替换 Draw 而期待驱动自动处理。

## 10. Present 与多视图

### 老引擎

`device_d3d11.cpp:2957-2978` 的 `DXGIXPresentArray` 已包含：

- SourceRect。
- DestRectUpperLeft。
- 横纵缩放因子。
- sync interval 和 immediate threshold。

这证明老 Durango 已有多源视图/缩放置换思想。

### 新引擎

- 内部 24 字节 plane 记录。
- 消费者展开成 `0x48`、align 8 的 XS plane。
- 每个 plane 带 frame token、资源、矩形、色彩空间和扩展描述。
- `PresentX` 在 direct queue 上执行，并与同一帧管线关联。

### 判断

新 PresentX 是老 PresentArray 的明确扩展：从 swapchain 参数扩展到 direct queue、多 plane、多视图、色彩空间和 token。`ScaleFilter` 虽保留 ABI，但 GDK 文档明确说 Scarlett 忽略。

## 11. 临时分配与生命周期

### 老引擎

`grcGfxContext::allocateTemp`（`gfxcontext_durango.cpp:1109-1197`）从当前 data segment 分配，生命周期由 allocation scope 管理。常量、即时顶点、fence scratch 都使用该机制。

### 新引擎

- 小上传使用线程 arena 和共享上传资源。
- 大上传创建独立临时资源并延迟释放。
- GCL/allocator 池化复用。
- 帧完成 fence 和延迟节点链控制最终回收。

### 判断

两者都在避免“每次上传创建独立资源”。新引擎的 arena 阈值、GPU 地址换算和 fence 延迟释放更显式，但不能把 `allocateTemp` 简单理解为 `AllocateFromCommandAllocatorX` 的同义词；后者在当前 EXE 中尚缺直接调用证据。

## 12. 关键误映射清单

| 不应直接等同 | 正确理解 |
|---|---|
| `grcDevice::EndFrame` ↔ `Gfx_EndFrameAndFlush` | 老函数含 Present；新函数主要负责 deferred 和资源生命周期 |
| `grcGfxContextCommandList::kick` ↔ `FrameSubmissionQueue_Consume` | 都是提交点，但新消费者还处理 token、回调、Present 和 fence |
| `grcFenceHandle` ↔ frame token | 老 fence 对应新 fence；token 是帧管线身份 |
| `grcStateBlock::Flush` ↔ `FlushFullState` | 都是 dirty 刷新，状态集合和 bit 定义不同 |
| COM SRV/UAV ↔ GPU descriptor handle | 一个是 COM view，一个是 heap/地址空间中的描述符 |
| `CopyOverTexels` ↔ `CopyTextureRegionX` | 前者是 CPU 侧 XG tiling，后者是 GPU 命令 |
| `allocateTemp` ↔ `AllocateFromCommandAllocatorX` | 都是临时分配思想，但当前新槽位缺直接实测，不能据此证明 |
| 老 XS/D3D11 槽位 ↔ 新 XS 槽位 | 不同 ABI，不能继承数值布局 |

## 13. 最终评价

新引擎的渲染接口在老 RAGE 设计上是合理且顺畅的演进：

- 保留多线程命令上下文、dirty-state、placement、临时分配和帧 fence。
- 将独立 shader/state 演进为 PSO。
- 将 COM view 演进为 descriptor/root binding。
- 将 D3D11 hazard 管理演进为显式 barrier/cache/queue 同步。
- 将 swapchain Present 演进为 direct queue 多 plane PresentX。
- 将段池和 allocation scope 演进为 GCL 池、异步提交环和 fence 延迟回收。

没有发现必须推翻当前核心 XS 布局或 RAGE 分层模型才能解释的矛盾。剩余不确定性仍在具体槽位、私有位域和个别参数，而不是整体渲染语义。
