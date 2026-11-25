# Scarlett 渲染语义到干净桌面 D3D12 的接口映射

## 0. 改造目标

本方案**不移植 GTA5 代码，不设置 D3D11 过渡后端**。工作内容是：

1. 以当前 EXE 恢复资源、program、命令、同步、帧和高级接口的真实合同。
2. 定义 API 无关的公共描述和强类型句柄。
3. 重新实现干净的桌面 D3D12 后端，使当前 pass 不再依赖原 XS 后端。
4. 最终覆盖当前实际使用的普通图形、compute、copy、indirect、query、mesh、StateObject/BVH 和多 queue。

当前 `game_scarlett_master.exe.i64` 决定目标语义。老 D3D11 和第三方项目不是新实现来源，也不能用来删减当前接口。

## 1. 替换边界

```text
保留：当前游戏高层 pass、场景/材质请求、deferred 调用意图
  ↓ 适配
ScarlettRenderInterface：API 无关资源/program/command/frame 合约
  ↓ 唯一新实现
NativeD3D12RenderEngine
  ↓ 替换
当前 PlatformRenderDevice、RageResource、Descriptor、PSO、GCL、提交与帧后端
```

公共接口不是 Device11/GCL vtable 的包装，也不是另一套老 grcDevice。后端内部使用原生 D3D12 PSO、descriptor、barrier、queue 和 fence，高层不接触 XS packet 或 COM 布局。

## 2. 设备、队列和显示

| 职责 | 当前 Scarlett 权威语义 | 干净桌面 D3D12 实现 |
|---|---|---|
| 设备 | `PlatformRenderDevice` 共享 Device11，并连接多个子系统 | Device 只负责创建与能力；资源/program/command/frame 分组件 |
| 上下文 | 每线程 `RenderThreadCommandContext` 懒绑定 GCL/allocator/pool | 每线程命令上下文，独立 allocator/list 和 fence 生命周期 |
| 图形队列 | 一个 direct queue，显式 Execute 和 Present | 一个 direct queue 起步，保持提交顺序 |
| 异步 compute | 四个 compute queue，各自队列上下文和池 | 四个 compute queue 及跨 queue fence，不串行冒充最终合同 |
| 列表池 | direct free-list、compute 加锁 round-robin、ResetX/Close | 池化 allocator/list，以 fence 控制 Reset 和复用 |
| 显示 | Win32、显示配置、视图资源和 Present 记录 | DXGI/Win32，保留视图、矩形和色彩语义 |
| Present | 80 字节记录最多两个内联 plane，消费者展开 XS plane 后 PresentX | 公共 PresentDesc 保留多 plane 与完成关系，桌面 swapchain/显式合成 |
| 重置 | 显示资源 deferred 销毁与重建 | resize、device removed、资源重建与在途回收 |

`QueueContextId`、pooled list index、queue index、frame serial 和 fence value 必须使用不同强类型，不能混为一个整数。

## 3. Shader、状态和绑定

| 职责 | 当前 Scarlett 权威语义 | 干净桌面 D3D12 实现 |
|---|---|---|
| 状态缓存 | requested/dirty/encoded，Draw 前 `FlushFullState` 或 root 子集刷新 | dirty 求差；静态状态进 PSO，动态状态调用标准命令 |
| Draw 前刷新 | topology、PSO、IA、OM、viewport/scissor、root、barrier 后 Draw | 保持同一因果顺序 |
| Program | `PipelineProgramKey` 描述 shader、IA、root、状态和 topology | 逻辑 program key，再生成桌面 PSO |
| Graphics PSO | 688 字节、15 个子对象 stream；派生状态另有 332 字节描述 | 标准 graphics desc/cache，不复制 XS stream ABI |
| Derived state | rasterizer→depth/stencil→可选 depth options→blend | 独立派生键和 PSO/cache |
| Compute PSO | 序列化 cache + CreateComputePipelineStateX fallback | 标准 compute PSO 和 shader hash cache |
| Descriptor | CPU/GPU handle、默认 descriptor 和 binding ID | 后端内部创建 descriptor，公共层只用逻辑 view/binding |
| 常量 | root constants、CBV、软件 16 字节硬件 CBV | 按频率选择 root constants/CBV，不复刻 tagged address |
| Root binding | 四组 root table、root SRV/UAV、root constants | 按 binding layout 生成 root signature |
| 输入布局 | PSO stream 中独立 input element scratch | 语义映射并保证数组存活到创建返回 |
| 空资源 | 默认 descriptor 类别和 binding ID 位域 | 类型正确的默认 SRV/UAV/CBV |

### Shader 独立工作流

必须恢复阶段、入口、输入/输出签名、binding layout、program 类型、编译缓存和 ready/failure 状态。当前打包格式已确认是 12 字节 `0xDCE7` 头加 Stored/raw-DEFLATE 载荷，解包不等于桌面 shader 可用；仍需从源码或明确中间表示编译到桌面 DXIL，不能假设 XS blob 可直接使用。只适配 Draw 而不解决 shader，不是全功能替换。

## 4. 资源、内存和复制

| 职责 | 当前 Scarlett 权威语义 | 干净桌面 D3D12 实现 |
|---|---|---|
| Buffer | `RageBuffer` 共享资源前缀，CPU address/CBV 存储可复用 | 逻辑 buffer 与后端存储分离 |
| Texture | `RageResource`、紧凑格式、多 plane/mip/slice、backing-only | 完整格式映射，允许 backing/resource/descriptor 不同生命周期 |
| 内存类型 | `Default/Upload/Readback/Custom` | default/upload/readback/custom heap |
| Placement | committed、forced placed、普通 placed、backing-only | 初版 committed 闭合，之后按需要 placed heap/aliasing |
| GPU VA | VA reservation、CPU alias、page pool、placement 地址转换 | 不伪造统一地址；需要地址的功能使用真实 D3D12 GPU VA |
| 页池 | 64 KiB 页、page mapping、scatter copy | 仅实际需要时使用 reserved resource/heap mapping |
| 上传 | arena 小上传、大独立临时资源、staging + GPU copy | upload ring + fence，按 plane/mip/slice/row 复制 |
| Readback | readback heap、GPU 完成与延迟可见 | fence 后映射，结果生命周期明确 |
| Copy | buffer/resource/texture copy，区分线性 CPU 位置和 D3D resource | 标准 CopyBufferRegion/CopyResource/CopyTextureRegion/Resolve |
| 回收 | fence、延迟节点和视图资源列表共同决定 | 统一 DeferredRelease，按 GPU fence 回收 |

禁止把所有资源放进 upload heap 模拟统一内存，也不能把 backing-only 的 `resource==nullptr` 当成错误。

## 5. 命令、barrier 和提交

| 职责 | 当前 Scarlett 权威语义 | 干净桌面 D3D12 实现 |
|---|---|---|
| Draw | 内联 24 字节 DrawIndexed 或 GCL 慢路径 | 标准 Draw/DrawIndexed/Instanced |
| Compute | compute PSO/root 变体 | 标准 Dispatch + compute root/PSO |
| Root 快路径 | 内联 root table/SRV/UAV/constants | 标准 SetGraphicsRoot*/SetComputeRoot* |
| Barrier | `PendingBarrierSet`、GCL146、组/资源容量 | 标准或 enhanced barrier，保持转换与 flush 边界 |
| Cache/pipe | SyncTop/BottomOfPipeX 和 cache flags | 用语义等价的 barrier/UAV barrier/fence，不复制 flags |
| Close/kick | Close、提交记录、消费者聚合最多 256 GCL | Close 后按当前顺序 ExecuteCommandLists |
| Partial submit | 段耗尽、record flush 和列表池冲刷 | 作为容量策略，不改变逻辑命令顺序 |
| 完成回调 | Execute 后按收集顺序执行，但不等于 GPU 完成 | 保留 CPU 回调位置，完成只由 fence 发布 |

新后端不需要 XS 原生 PSO packet、GCL 写指针或 root opcode。

## 6. 高级功能

| 当前功能 | 当前已确认合同 | 桌面 D3D12 实现任务 |
|---|---|---|
| ExecuteIndirect | 5 个 signature、9 个包装，独立/共享参数缓冲 | 标准 signature、argument/count resource 和 offset，逐包装验证 |
| QueryHeap | occlusion/timestamp/CPU-only 分类 | heap、Begin/End/Resolve/readback；动态确认实际录制链 |
| StateObject | N+9 个子对象、Properties、identifier | 桌面 DXR state object，按逻辑结构重建 |
| DispatchRays / SBT | 创建已恢复；主 GCL 表未见 SetPipelineState1/DispatchRays 实调用 | 静态实现创建合同；动态命中时再恢复 record 地址/stride/大小、维度、Scratch/ExtraOptions 和异步 ready，不虚构 SBT |
| Direct DispatchMesh | 标准 0x408 与私有 0x320 均未见实调用；AS/MS PSO 创建绑定已恢复 | 保留间接三个 uint32 XYZ；direct 及特定 barrier 动态命中再补，不虚构 |
| BVH build | 最多 16 项批量描述、update/scratch/compaction | 标准 BuildRaytracingAccelerationStructure，保持同步 |
| BVH copy | COMPACT、compacted-size 复制方向 | 标准 copy + compaction 同步 |
| Rehydrate | 56 字节标准序列化头和 XS 路径 | 等价反序列化/构建流程，不伪造同名空调用 |
| PrebuildInfo | bottom-level prebuild 查询 | 标准 prebuild 查询和 scratch 生命周期 |
| Morton 模式 | `Set64BitMortonCodes` 开关后恢复 | 在输入生成/构建配置中实现等价要求 |
| AS/MS | graphics stream 复用 VS/HS 子对象槽 | 标准 mesh/amplification PSO |
| 多 queue | 1 direct + 4 compute | direct/compute queues、跨 queue fence 和资源同步 |

未决项必须保留证据边界；私有快路径未恢复不阻碍使用公开桌面 D3D12。

## 7. 帧、deferred 和同步

| 职责 | 当前 Scarlett 权威语义 | 改造要求 |
|---|---|---|
| BeginFrame | 视图选择、帧状态和线程命令上下文协同 | 默认状态与旧帧回收，不只等待 Present |
| EndFrame | `Gfx_EndFrameAndFlush` 执行 deferred 和显示资源生命周期 | 按因果顺序拆接，不按函数名替换 |
| Deferred | 64 个注册闭包、48 个直接入口、pending 清除但注册保留 | 保留注册、去重、pending 和执行顺序 |
| Frame scope | BeginFrameScope/EndFrameScope 和 pending 状态 | 明确 scope depth，控制提交和资源操作 |
| Frame token | 标识提交和 Present 的帧管线归属 | CPU frame serial/提交归属，不伪造 XS token API |
| Fence | Queue Signal 表示 GPU 完成 | 独立 FencePoint，驱动 allocator/list/resource 回收 |
| 完成状态 | packed 计数信号量、批次完成状态和等待计数 | 保留生产/消费完成语义，避免提前复用记录 |

## 8. 新模块

- `RenderResourceDevice`：格式、资源、view、上传和回收。
- `RenderProgramManager`：shader 编译、program、PSO 和 binding layout。
- `RenderCommandContext`：线程状态、普通和高级命令。
- `RenderSubmissionQueue`：命令聚合、回调、wait/signal 和 fence。
- `RenderFrameCoordinator`：frame scope、deferred、Present、视图和重建。
- `RaytracingDevice`：StateObject、Properties、SBT 和 BVH。
- `BackendCapabilities`：只用于硬件能力检查和中间诊断，不永久删除功能。

## 9. 顺序与验收

1. **公共描述和句柄**：脱离 XS COM/vtable。
2. **D3D12 核心**：设备、资源、descriptor、queue、fence、swapchain。
3. **普通 pass 接入**：Draw/Dispatch/Copy/Present/deferred 全走新边界。
4. **shader/program 链**：完整编译、缓存和失败传播。
5. **高级路径**：Indirect、Query、AS/MS、StateObject/BVH、多 queue。
6. **全功能验证后优化**：不改变公共语义。

最终验收逐 pass 证明功能存在并实际执行，同时检查资源内容、同步和生命周期，不能只证明“未崩溃”。
