# 干净桌面 D3D12 渲染引擎实现清单

## 0. 清单用途

本清单定义**全新实现**需要编写的组件，不再列 GTA5 文件，也不包含老 D3D11 后端提取步骤。所有目标语义来自 `game_scarlett_master.exe.i64`；老项目仅保留为历史交叉分析材料，不进入代码基线。

优先级：

- **P0**：普通渲染闭合前必须完成。
- **P1**：全功能替换必须完成，但可在普通路径稳定后实现。
- **OPT**：正确性之后按测量决定，不是初始正确性的前提。

以下为建议的新模块边界，不表示这些实现文件已经存在。

## 1. P0：公共类型与设备核心

| 新组件 | 必须实现的职责 | 主要验收 |
|---|---|---|
| `RenderTypes` | 格式、资源/program/binding 描述、强类型句柄、命令参数、错误类型 | 不暴露 XS/D3D11 ABI；无整数句柄混用 |
| `RenderDevice` | adapter、D3D12 device、feature 检测、debug 信息和设备移除 | 创建/销毁/移除恢复明确，无假成功 |
| `SwapchainPresenter` | DXGI swapchain、back buffer、Present、resize、色彩与多 plane 请求 | Present 顺序正确，resize 稳定，辅助 plane 不被静默丢弃 |
| `QueueManager` | direct queue 起步，FencePoint、wait/signal 和完成查询 | frame serial 与 GPU fence 不混淆 |
| `CommandAllocatorPool` | allocator/list 获取、Close、Reset 和在途保护 | fence 前不复用，线程之间不共享错误上下文 |
| `DeferredReleaseQueue` | heap/resource/descriptor/staging 的延迟回收 | 压力测试无提前释放和持续增长 |

## 2. P0：资源、描述符与内存

| 新组件 | 必须实现的职责 | 主要验收 |
|---|---|---|
| `ResourceFactory` | buffer/texture、committed resource、初始数据、mip/slice 和 backing-only 形态 | 描述逐字段匹配当前合同 |
| `HeapMemoryManager` | default/upload/readback/custom heap、分配统计和所有权 | 不把所有资源放 upload heap |
| `UploadArena` | 小上传 ring、大独立 staging、按 plane/mip/row 组织 GPU copy | 在途区域不覆盖，大上传不阻塞错误路径 |
| `ReadbackManager` | readback resource、fence、映射与结果生命周期 | 映射前确认 GPU 完成；水面物理使用三槽、非阻塞旧结果查询，详见 `WATER_SIMULATION_READBACK.md` |
| `DescriptorAllocator` | CPU/GPU heap、SRV/UAV/CBV/RTV/DSV、默认 descriptor | 类型、格式和绑定 ID 对应，无空指针冒充 |
| `ResourceStateTracker` | layout/access/sync、barrier 合并、UAV 和跨 queue 可见性 | PIX/GPU validation 无状态错误 |

OPT：placed heap、reserved resource、aliasing、页映射和更激进的内存复用。只有功能需要或测量获益时实现。

## 3. P0：shader、PSO 和普通命令

| 新组件 | 必须实现的职责 | 主要验收 |
|---|---|---|
| `ShaderCompiler` | 源码或明确 IR 到桌面 DXIL，阶段/入口/签名、同步/异步 ready 和失败传播 | 当前每类 program 有可用编译路径 |
| `ProgramManager` | 逻辑 program key、binding layout、编译缓存和生命周期 | 相同键稳定命中，不同状态不错误共享 |
| `RootSignatureManager` | root constants、四组 table、root SRV/UAV 和默认绑定 | 与当前 binding layout 逐项一致 |
| `PipelineCache` | 688 字节 stream 的逻辑等价 graphics desc、compute 和派生 PSO | 固定状态完整，输入元素存活到创建返回 |
| `RenderCommandEncoder` | Draw/Dispatch/Copy、IA/OM/viewport/scissor/root/barrier | Draw 前刷新顺序正确，无静默空操作 |
| `FrameCoordinator` | Begin/End scope、deferred、视图、提交和 Present 因果顺序 | 不错帧，回调不被误当 GPU 完成 |

Shader 工作流必须与 C++ 后端同时推进。若 mesh/DXR 没有可用源码或可转换 IR，应明确阻塞，不用旧 shader 或空 shader 冒充完整功能。

## 4. P1：高级接口

| 新组件 | 必须实现的职责 | 主要验收 |
|---|---|---|
| `IndirectCommandManager` | 5 个 signature、9 个包装、独立/共享参数缓冲和标准 resource+offset 适配 | stride、参数内容和 DispatchL2 语义逐项验证 |
| `QueryManager` | occlusion/timestamp heap、Begin/End/Resolve/readback；CPU-only 类保持独立 | 实际录制链与结果时序正确 |
| `MeshCommandEncoder` | AS/MS PSO、direct DispatchMesh 和间接 DispatchMesh | 参数格式、绑定和 barrier 正确 |
| `RaytracingPipelineManager` | N+9 逻辑子对象、Properties、identifier 生命周期和异步 ready | export/association/config 与当前合同一致 |
| `ShaderBindingTableBuilder` | raygen/miss/hit-group record、stride、大小、CPU/GPU 所有权 | StateObject 存活期间记录有效，更新不踩的在途 SBT |
| `RaytracingCommandEncoder` | SetPipelineState1、DispatchRays、Width/Height/Depth 和必要的 XS 扩展语义适配 | 真实 rays 执行且结果正确 |
| `AccelerationStructureManager` | build/update/scratch/compaction/copy/rehydrate/prebuild 和 Morton 输入要求 | 80 字节批量逻辑、UAV/COPY_SOURCE 与复制方向正确 |
| `MultiQueueScheduler` | 1 direct + 4 compute、跨 queue fence、列表池和资源同步 | 无竞争、提前回收或顺序偶发错误 |

若后续证明某个高级接口在此 EXE 中只有创建而没有执行，应保留明确证据，并将验收分成“对象创建合同”和“当前未命中执行合同”，不能伪造游戏内调用。

## 5. 推荐实现顺序

1. `RenderTypes`、设备、queue、fence 和 deferred release。
2. committed resource、descriptor、upload/readback 和 state tracker。
3. 最小 shader/PSO、Draw/Dispatch/Copy、swapchain 和 frame scope。
4. 当前普通 pass、格式、program/binding 和 deferred 全量接入。
5. Indirect、Query、AS/MS、DXR/BVH 和多 queue。
6. 全量替换后进行内存、缓存和提交优化。

每一步保留可运行版本和回归场景。中间可以暂时报告“不支持并失败”，不能返回成功但不录制命令。

## 6. 验收物

- 新引擎源码及模块依赖清单，不含 GTA5 提取文件。
- 可独立运行的桌面 D3D12 普通渲染路径和真实游戏接入版本。
- 当前 shader/program/binding 的编译与缓存清单。
- 原后端与新后端的资源、命令、readback、Present 和回收对照日志。
- 普通与高级功能的真实场景结果、PIX capture、debug layer/GPU validation 记录。
- 长时间运行、resize、设备移除、在途回收和内存压力测试结果。

本文只定义实现任务。现阶段没有新引擎编译或实机运行结果。
