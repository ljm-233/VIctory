# 非功能性证据边界（当前 title 静态合同已闭合）

本目录已覆盖设备、资源、格式、描述符、命令、提交、PSO、帧控制和当前 EXE 的高级对象创建。**本轮按原始地址复核后，当前 title 已识别的静态接口合同已经闭合。** PSO hash、完整 root-signature bitstream、packed page commands、packed geometry/instance 和分块 render-target transition 均已补齐。下列内容是名称、私有实现、未调用扩展或实机证据边界，不再构成静态功能缺口。

## 本轮已解决的实质静态缺口

- `0x140F2C470` 的 packed page commands 与页索引/free-list 主算法已恢复。
- `0x140075480`、`0x144EE2A00` 的 packed BLAS/TLAS 输入转换已恢复。
- `0x140032520` 的逐 RT/DS 分块 transition 修复算法已恢复。

详细修正与地址映射见 `RENDER_REMAINING_AUDIT.md` 第 10 节。以下未调用接口和私有名称仍按原证据边界处理，不与已解决的缺口混淆。

## 高级执行路径的最终分类

- **DXR**：`0x1424785AD` 明确 `DisableDXR=TRUE`。StateObject、Properties、identifier 和异步创建已恢复；全 `.text` 审计未证实 SetPipelineState1/DispatchRays/DispatchRaysX。当前没有 shader binding table、XYZ 维度、Scratch/ExtraOptions 或异步执行等待合同，不能虚构 SBT。
- **Direct DispatchMesh**：标准 0x408 与私有 0x320 的全 `.text` 候选均未形成直接执行证据；实际入口是 ExecuteIndirect。AS/MS 的 PSO 创建、ready 和普通 SetPipelineState 绑定已恢复，direct 特定 barrier 不属于当前活跃合同。
- **Query**：4 个 CPU-only 对象、4 个 occlusion heap 和 1 个 timestamp heap 均只有创建与 shutdown 引用，未确认 BeginQuery、EndQuery、ResolveQueryData 被录制。创建/销毁合同已闭合，不伪造活跃查询流程。

这些分类不排除动态配置改变后重新命中；届时按真实调用补，而不是把当前静态结果写成运行证明。

## 本轮已闭环的主要内容

- `SubmissionRecord +0x28` 为 `flushAndExecutePooledList`；Present 最多两个内联 plane，矩形、间隔和完成关系已恢复。
- packed synchronization state 的 token、自旋预算和计数位已恢复，只有 bit31 保留，不影响替换。
- 688 字节 graphics PSO stream 全部 15 个子对象、AS/MS 复用 VS/HS、派生 PSO 四类描述均已恢复。
- 48 个 deferred 入口顺序和直接行为已恢复；30 项保留 `RenderPassXX` 名称，但不影响按原顺序执行。
- StateObject 为 N+9 个子对象，export、root association、16/8 字节 shader config、递归深度 1 和 identifier 生命周期已恢复。
- ExecuteIndirect 为 5 个签名和 9 个实际包装，独立/共享参数、DispatchL2 和 DispatchMesh 均已区分。
- BVH 80 字节批量描述、update/scratch/compaction、COMPACT、56 字节标准序列化头、UAV/COPY_SOURCE 和 compacted-size 复制方向均已恢复。
- Shader blob 为 12 字节 `0xDCE7` 头、codec 3 Stored、codec 13 raw DEFLATE；compute root 与 NodeMask 已纠正。
- `RageFormatDesc` 的 12 字节完整布局、DXGI_FORMAT 直映射、dimension 和 17 项 DWORD sampleMode 表已恢复。
- 分配器扩展/主路径分别固定 variant 1/3，释放同槽；初始化 0/2/4/5 为专用实现、1/3/6/7/8 为可替换通用实现。
- GCL shading-rate 三连已实测：0x3F0 `RSSetShadingRate`、0x3F8 `RSSetShadingRateImage`、0x400 `RSSetShadingRateX`。后者 Type=PIXEL_CENTER_OFFSETS，不能与 Device 同偏移资源创建混淆。

## XS 接口槽位的非功能性边界

- **Device 126**：当前 EXE 没有符合资源创建形状的调用，不能独立命名。它是未使用的 XS 扩展/保留项，不参与当前 title 替换；不能猜成 Placed2。
- **Device 128/129**：最佳布局为 `CreatePlacedResource2 / CreateReservedResource2`，顺序来自 PC 代际模型；此 EXE 无实际调用，不能升级为二进制实测。
- **Device 130/131**：最佳布局为 `CreateComponentPlacedResourceX2 / EnableLiveDebuggingX`，此 EXE 无实际调用。
- **Device 132**：`ReportGpuHaltX(device,3)` 已由 `0x1424745E4` 和设备别名 `0x142478621` 证实。flags 名称和返回类型未证明，但返回被忽略；替换时不得凭忽略 EAX 反推 `void`。
- **GCL 144/145、147/148**：文档最佳名称分别为 EnableLiveDebuggingX、SetComputeShaderLaunchParametersX、OMSetFrontAndBackStencilRef、AllocateFromCommandAllocatorX；当前 EXE 未形成活跃调用合同，GCL146 Barrier 已实测，不再混淆。
- **`CreateDeviceFlags=0x20`**：本地旧头文件没有对应枚举名，替换时保留数值，不映射成其他 flag。
- 未调用的 XS 方法不应为了覆盖头文件而生成伪调用。

## 对象布局与同步

- packed synchronization state 的 bit31 保持 reserved；其余位和计数行为已确定，底层等待节点池不属于渲染替换必须展开的范围。
- `PresentationViewState` 的 72 字节步长和四组 per-view pending-list 状态已确定；少量锁/保留字段不虚构名称。
- `CommandListEncodingState` 只声明已证实字段；保留区不影响命令包生成与慢路径回退。
- direct/compute 列表槽 5..10 只有保留和 shutdown 释放语义，初始化实际创建的是槽 0..4。

## 硬件命令编码与私有生成器

- Draw、IA、root packet 的 opcode 和选择规则已命名；未公开硬件位域按原始掩码/字节复制，不拆成猜测字段。
- 软件 CBV 的 16 字节编码公式已恢复；`flags=0x2403C` 的逐位硬件含义不影响逐字节替换。
- root descriptor-table 写 handle 低 32 位，root SRV/UAV 写完整 64 位地址，二者不能混用包结构。
- `NativePsoInlineData` 为 32 字节：16 字节 prefix、8 字节 stateAndFlags、4 字节 tail、4 字节 shadowOnly。前 28 字节进入命令包，全部 32 字节进入 shadow。
- 私有 PSO 字节的生成算法位于 XS 运行库而非游戏 EXE；当前没有 `d3d12_xs.dll` 可继续展开。标准桌面后端使用 SetPipelineState，不需要复制该生成器。

## 资源与格式

- `RageResource` 的 flags 只有已验证位被命名，其余位按原始掩码处理。
- backing-only 路径允许 `resource==nullptr`，是原始控制流的真实行为。
- 页池物理页 class/tag 保留观察常量，不杜撰成 XMem 枚举。
- 部分原始 pass 名、完整原始锁名和私有字段名未恢复，但其直接行为、参数和生命周期已闭合，不影响功能替换。

## 验证限制

- 当前闭合结论是**静态接口合同**，不是运行游戏、PIX capture 或 Windows 实机验证。
- 伪代码用于解释 ABI、状态和控制流，不是可直接编译的实现。
- 干净桌面 D3D12 后端和桌面 DXIL 编译链仍属于后续实现工作；主要静态合同的恢复程度不等于实现已经完成。
- 水面专题已恢复 Compute、CPU alias、三种采样模式、权重插值和双三槽索引，但每次 dispatch 的原生资源与每次物理查询的 alias 尚需动态地址对照，见 `WATER_SIMULATION_READBACK.md`。
