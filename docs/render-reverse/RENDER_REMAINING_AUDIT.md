# 渲染接口静态闭合与剩余缺口审计

## 0. 审计口径

本文按“完整替换当前 title 所需功能”评估，不以是否知道每个私有名称判断。状态分为：

- **静态闭合**：参数、布局、行为、状态和生命周期足以实现替换。
- **创建闭合、执行未证实**：对象创建存在，但当前配置/二进制没有执行消费者。
- **非功能性保留**：未调用扩展、原始名称或运行库私有算法，不影响当前替换。
- **实机待验证**：静态合同已覆盖，但仍需真实运行。

**本轮逐地址复核后的结论：当前 title 已识别的渲染接口层达到完整静态合同闭合。** 原先未展开的 PSO hash、完整 root-signature bitstream、packed page commands、packed geometry/instance 和分块 render-target transition 均已补齐。该结论仅覆盖本 EXE 已识别的静态活跃合同；水面逐资源生产者/消费者对应仍需动态对照，新后端实现和实机验证另算。

## 1. DXR：创建闭合，当前配置禁用执行

StateObject 创建链已恢复：

```text
DXIL library + hit groups + root signatures
→ CreateStateObject
→ Properties
→ raygen/miss/hit-group identifiers
```

设备初始化在 `0x1424785AD` 明确设置 `DisableDXR=TRUE`。全 `.text` 线性反汇编中，主 GCL 表的 `SetPipelineState1=0x3D0`、`DispatchRays=0x3D8`、`DispatchRaysX=0x430` 分别得到 42、28、31 个真实候选，但没有 StateObject、Shader binding table 和 XYZ 维度的执行组合。

因此：

- **SetPipelineState1 / DispatchRays / DispatchRaysX：未证实此 EXE 实际调用。**
- **Shader binding table：没有可归属的运行时布局证据。** record 地址、stride、大小、CPU/GPU 所有权、Width/Height/Depth、Scratch/ExtraOptions 均不能虚构。
- N+9 创建描述没有 callable 子对象；未来若执行该 pipeline，callable table 应为空，但当前未观察到 callable table 描述。
- `0x142480180` 仅创建并释放异步任务；未找到消费者，异步执行等待关系不属于当前合同。

静态替换应实现已恢复的 StateObject 创建语义。动态 Hook 若改变配置并命中 DispatchRays，再补抓 SBT、描述字节和等待关系。

## 2. Direct DispatchMesh：未证实，AS/MS 创建绑定闭合

标准 `DispatchMesh=0x408` 的 337 个真实间接候选和私有 `DispatchMeshX=0x320` 的 34 个候选均未形成当前 GCL 的直接 mesh 执行。实际确认入口是 `0x144E33E10` 的 ExecuteIndirect 包装。

AS/MS 已明确：`UsesMeshShaderPipeline()` 改写 VS/HS 子对象类型后，复用普通 graphics program 的 PSO cache、ready 和 SetPipelineState 流程。桌面间接记录为三个 `uint32` 的 XYZ，共 12 字节；私有 `D3D12XBOX_DISPATCH_MESH_X_ARGUMENTS` 不是当前 direct 原型证据。

direct 调用及特定 barrier 属于动态待验证项，不再列为静态功能缺口。

## 3. Query：创建销毁闭合，执行未证实

RAGE kind 映射已确认：

| kind | 实际行为 |
|---:|---|
| 1 | 创建 `D3D12_QUERY_HEAP_TYPE_OCCLUSION` |
| 2 | 只分配 CPU 结果数组，不创建 D3D12 QueryHeap |
| 4 | 创建 `D3D12_QUERY_HEAP_TYPE_TIMESTAMP` |

逐一检查全局对象：

- `0x14A943A18..0x14A943A30`：4 个 CPU-only 对象，只有初始化与 shutdown。
- `0x14A943A38..0x14A943A50`：4 个 occlusion heap，只有初始化与 shutdown。
- `0x14760AEE8`：timestamp heap，只有 `Subsystem_FrontendBoot` 创建与 `0x142497A90` 销毁。

当前未确认 BeginQuery、EndQuery、ResolveQueryData 被录制。创建/类型/销毁合同已经闭合，不能写成活跃查询链已执行。

## 4. Bundle 与其他标准入口

`ExecuteBundle` 没有形成已确认的实际使用合同。初版后端可直接录制主命令列表；若动态命中再补。

未被当前 EXE 调用的 XS 槽位不应为了覆盖头文件而全部虚构实现。它们与“当前 title 必须运行的接口”分开管理。

## 5. 保留 ABI 项的最终处理

- Device 126 为未使用扩展/保留项，无法独立命名但不参与替换。
- Device 128/129 最佳布局为 Placed2/Reserved2；130/131 最佳布局为 ComponentPlacedX2/EnableLiveDebuggingX，均无实际调用。
- GCL 144/145、147/148 保留文档名称和证据等级，当前未形成活跃合同。
- `CreateDeviceFlags=0x20` 保留数值；`ReportGpuHaltX` 的 flags 名称与返回类型不猜。
- direct/compute 列表槽 5..10 只有保留与 shutdown 释放语义。
- 原生 PSO 32 字节布局和使用方式已恢复；生成算法在 XS 运行库，不在游戏 EXE，标准桌面后端无需复制。
- packed synchronization state 的 bit31、部分锁/保留字段和私有硬件位域按原值处理，不拆成猜测名称。

这些不再构成当前 title 的静态功能缺口。

## 6. 命名与高层语义边界

- 30 个 deferred 回调只有行为名，原始 pass 名未确认，但直接行为与执行顺序已知。
- 9 个 ExecuteIndirect 包装的原始 pass 名未全部确认，参数和调用语义已知。
- 部分分配器、视图和锁的原始名称未知，字段和生命周期已确定。
- `0xDCE7` blob 的 Stored/raw-DEFLATE 解包已恢复，但解包不等于桌面 shader 可用；桌面 DXIL 编译链属于后续实现。
- 干净桌面 D3D12 引擎按 `CLEAN_RENDER_BACKEND_PLAN.md` 新写，不提取 GTA5 代码。

这些不妨碍静态接口闭合，但会阻塞最终“全功能、同画面”的实现验收。

## 7. 实机验证缺口

静态闭合不替代：

- Windows Hook 或独立后端真实运行。
- PIX capture、GPU validation 和逐 pass 对照。
- DXR/SBT、direct DispatchMesh、Query 在配置改变后的实际命中证据。
- 资源在途回收、Present、resize、设备移除和长时压力测试。
- 快慢路径一致性与性能测量。
- 水面每次 Compute dispatch 的原生资源与每次物理查询 alias 的动态地址对照，见 `WATER_SIMULATION_READBACK.md`。

## 8. 整体交叉复核修正

本轮重新以 IDB 调用点为准，对照伪代码、XS 布局 JSON、头文件和文档，修正了以下实质问题：

- `RageFormatDesc::sampleMode`：`0x140C4B05F` 的边界是 `sampleMode-1 <= 0x10`，`0x1461EDB14` 是 17 个 DWORD；删除伪代码中不存在的第 18 项。
- `CopyPageMappingsBatchX`：`0x140F2CC7F` 明确传入 `SourcePagePoolPageCount`，scatter 和 batch 使用独立数组及计数；补回参数，不再把物理页数组当 batch 描述。
- CPU→buffer 上传：`0x1404F45FD` 的源参数是 arena 偏移，大上传路径也使用资源内附加偏移；`CopyBufferRegion::SrcOffset` 不是 GPU VA。
- tools 内存迁移：`0x142216062..0x142216095` 使用 KernelPool 条件，并从第二份统计的 `ToolsLimit-ToolsUsed` 计算迁移量，调用 `XMemTransferToolsToSystem`；不再虚构统一的 `Required` 差值。
- 派生 PSO：`0x14247A096` 比较的是缓存项的 base PSO，返回的是另一字段保存的派生结果，修正两者混用。
- DXR StateObject：`0x1424813F9..0x142481460` 的两次 HRESULT 检查均有失败返回，补齐失败分支。
- 文档同步：列表槽 5..10 统一为“只有保留与 shutdown 释放语义”；Submit 回调按实际汇编直接调用，不再写成可选回调；补齐 Device101、Queue16/31/32/35、GCL61 等布局条目的二进制证据标记。

这些修正不改变已实测的 Device/GCL/Queue 槽位编号。后续逐地址复核又发现新的实质差异，因此旧“静态闭合”结论不再保留；结构验证也不能替代语义复核。

## 9. 后续顺序

1. **桌面 shader/DXIL 编译链**：复用已恢复的 PSO 与解包合同。
2. **干净 D3D12 核心实现**：先闭合普通图形、compute、copy、帧和回收。
3. **高级功能实现**：按已恢复合同实现 Indirect、AS/MS、StateObject/BVH 和多 queue。
4. **动态补证**：仅在实际命中时补 DXR/SBT、direct mesh、Query、Bundle 和未调用扩展。
5. **私有优化**：最后处理，不阻塞普通接口改造。

因此，主要接口恢复可进入后端设计阶段，但完整替换前仍需完成剩余 `partial` 模块；后续也不能用虚构未观察执行路径的方式填洞。

## 10. 本轮按原始地址对照伪代码的专项复核

本轮不再以“存在 `sub_` 注释”作为完成证明，而是逐函数比较反编译/反汇编与伪代码。已修正的实质差异包括：

- `0x140032130`：恢复 root table 的首次分配、首次编码、四组 dirty 掩码和 `state & 0x10000` 强制重放；不再用简单的地址不等谓词代替。
- `0x14248CE00`、`0x140032520`、`0x140032130`：快路径检查统一还原为原指令的 `writeCursor < limit`，不虚构全 packet 容量检查；PSO 仅在实际换绑时重绑，VB 使用真实 count。
- `0x1400095E0/0x1400097F0/0x1400099F0`：补回 40 字节 buffer 和 64 字节 texture/range barrier 的完整字段生成；`0x1400320E0` 自身不做 count==0 提前返回。
- `0x144D9EFD0`：补回 SRV/CBV dirty 清除与重建，重建失败回退默认描述符。
- `0x140CD7320/0x140CD72B0`：拆成记录生产与 frame-tracking token 包装；Present 补回 DISPLAY_FLIP 的无限等待，completion fence 和记录 serial 分开。
- `0x141CF3280`：external CPU base 恢复为外部分配作用域，而不是创建后仅比较地址。
- `0x14193DE10`：backing-only 保留已有地址；forced-placed 保留原返回行为；失败释放检查资源当前所有权位。
- `0x14247FD30`：compute 创建失败释放 root 引用但不清空字段、不提前返回，仍执行统一收尾。
- `0x142481AA0`：补回 CPU-only 结果数组、结果 buffer 分配查询和失败释放。
- `0x140FE4E50`：64 位 Morton 路径不再误用 update scratch；`0x140BD96A0` 补回每项后的条件性工作冲刷。
- `0x140EA6C60`：Source 使用原始地址，CPU 解析和 originalInstanceData 使用 alias，二者不再混写。
- `0x142215FD0`：迁移量修正为 `min(AlignUp(availableTools,2MiB),16MiB)`，不是 max。
- `0x140047640`：补回 mode 0、mode 1、其他非零模式，以及权重表、逐查询变体、批量输出和 Inf/NaN 清理；`+0x28` 按 GPU/CPU 路径区分原生资源和权重表。
- `0x140BA1240`、`0x142625EE0`、`0x141FB4F70`：补回 Batch 常量、compressed 多次调度、可选组合和第二个三槽索引。

### 10.1 九个 ExecuteIndirect 的真实地址映射

| 地址 | 正确语义 | 参数形式 |
|---|---|---|
| `0x14248D000` | Draw | 独立资源 |
| `0x14248D1B0` | Draw | 共享缓冲 |
| `0x14248D350` | DrawIndexed | 独立资源 |
| `0x14248D500` | DrawIndexed | 共享缓冲 |
| `0x14248CA90` | Dispatch | 独立资源 |
| `0x14248CBE0` | Dispatch | 共享缓冲 + 附加字节 |
| `0x140094360` | DispatchL2 | 独立资源 |
| `0x14248CD20` | DispatchL2 | 共享缓冲 |
| `0x144E33E10` | DispatchMesh | 独立资源 |

伪代码此前把其中多个地址接到了错误函数名，现已按实际 signature 全局对象全部重接。独立参数大小保留原 32 位截断，公共 XS 调用不再重复执行各包装自己的状态冲刷。

### 10.2 原先未展开模块的最终处理

- `0x140F2C470`：已恢复六种 packed page commands、21 位页索引/free-list、scatter/batch、排序和 VA 去重。
- `0x140075480`：已恢复 TOP_LEVEL instance 与 BOTTOM_LEVEL triangles/AABBs、transform 复制、alias、compaction 和 update scratch；`0x144EE2A00` 的几何转换也已展开。
- `0x140032520` 的 render-target 段：已恢复 `sub_144DBD6C0/sub_144DBD700/sub_144DBE170/sub_144DBE210` 对应的范围生成、格式映射、10-bit 状态记录和单记录/全不同/混合处理。
- `0x1455AC630` 与 `0x140B5A5D0/0x142479760`：完整 root-signature bitstream 和两套独立 hash 已恢复。

上述原先的静态缺口已经补齐。结构验证脚本仍只检查交付物一致性，不替代原始地址复核、游戏运行或 PIX 验证。
