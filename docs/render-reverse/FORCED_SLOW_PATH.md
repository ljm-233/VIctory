# 强制渲染慢路径（降级）方法

目标是在不修改游戏 EXE 指令的前提下，让所有已识别的 GCL 内联编码和 PSO 快路径进入 XS 函数表调用，先建立正确性基线。此处描述的是替换对象/Hook 的实现方式，不是对当前 IDB 打补丁。

## 1. GCL 统一降级

在自定义 GCL 对象中，把从 `+0x18` 开始的五个 64 位 put/limit 字段全部置零：

| 偏移 | 字段 |
|---:|---|
| `+0x18` | put/write cursor |
| `+0x20` | plain limit |
| `+0x28` | draw/soft limit |
| `+0x30` | API/allocation limit |
| `+0x38` | unbiased API limit |

```cpp
void ForceSlowPath(CommandListEncodingState& list) {
    list.writeCursor = nullptr;      // +0x18
    list.plainLimit = nullptr;       // +0x20
    list.softPacketLimit = nullptr;  // +0x28
    list.allocationEnd = nullptr;    // +0x30
    list.unbiasedApiLimit = nullptr; // +0x38
}
```

效果是 `put < limit` 永远不成立：

- topology → `IASetPrimitiveTopology`
- PSO → `SetPipelineState`
- index buffer → `IASetIndexBuffer`
- vertex buffers → `IASetVertexBuffers`
- stencil ref → `OMSetStencilRef`
- root table/SRV/UAV/constants → 对应 root setter
- DrawIndexed → `DrawIndexedInstanced`
- ExecuteIndirect 本身直接调用 GCL slot 61，不使用 DrawIndexed 内联包，仍应记录为独立慢速入口

参考实现 `VIctory-Belobug-AMD/crates/victory-gpu/src/list.rs:98-105` 也采用相同方法，将五个 qword 全部置零，使所有内联检查失败。

### 必须在创建和 Reset 时重复应用

不要只在设备初始化时处理一次。GCL 池会调用 `ResetX`，可能重建运行库的写指针和限制状态：

```cpp
ID3D12GraphicsCommandList6* AcquireList(CommandListSlot& slot) {
    auto* list = slot.list;
    xs::FunctionTable<...>(list)->ResetX(
        list, slot.allocator, nullptr, D3D12XBOX_COMMAND_LIST_RESET_FLAGS_NONE);

    ForceSlowPath(*GetEncodingState(list)); // 每次 ResetX 后重新置零
    return list;
}
```

如果池已经在运行，应在列表未录制、未提交且没有 GPU 消费者引用的阶段应用，避免与命令写入竞争。

### 不要清零整个 GCL 前缀

必须保留：

- `m_pFunction` 和 GraphicsUnknown 引用计数；
- list 类型和所属 allocator；
- `+0x544` PSO shadow 的可写存储；
- root opcode 表及运行库私有状态。

PSO 的 32 字节 shadow 在慢路径也会复制，所以 `+0x544..+0x563` 必须保持有效。只让 put/limit 检查失败，不能让整个对象变成空指针。

## 2. PSO 内联绑定的降级

PSO 使用相同的 allocation limit，因此上述置零会让 `0x1400327AA` 的快路径失败并调用 `SetPipelineState`。

无需伪造原生 PSO `+0x10..+0x2F` 的 32 字节命令数据来让慢路径工作；这些数据仍可能用于 shadow/诊断，应保持对象可写，但不要求命令包编码正确。

如果只想单独降级 PSO 而不影响其他内联包，可以在 `EncodePipelineStatePacketOrCall` 的 Hook 中直接转调：

```cpp
void EncodePipelineStatePacketOrCall(
    CommandListEncodingState* list,
    ID3D12PipelineState& pso) {
    WritePipelineShadowIfRequired(list, pso);
    xs::FunctionTable<...>(list)->SetPipelineState(list, &pso);
}
```

## 3. 软件 CBV 必须单独处理

GCL limit 不影响 descriptor 创建。软件 CBV 仍会执行：

```cpp
raw->taggedAddress = desc.BufferLocation + (1ULL << 52);
raw->recordCount = AlignUp(desc.SizeInBytes, 16) / 16;
raw->flags = 0x2403C;
```

如果降级目标是“所有可简化的路径都调用 XS”，需要额外启用原生 descriptor 创建：

```cpp
descriptorFactory.useNativeDescriptorCreation = true;
```

或者使用与当前设备 `+0x18` out-of-line 模式兼容的开关。不要只清空 GCL limit 后宣称软件 CBV 也已降级。

原生 CBV 调用：

```cpp
device->CreateConstantBufferView(&desc, destination);
```

## 4. 不受降级影响的部分

以下机制不是 GCL 内联编码，不能通过 put/limit 关闭，也不应为了“慢路径”而删除：

- GPU VA/CPU alias、页池和资源地址所有权；
- descriptor bump/free-list；
- direct/compute GCL 池；
- PSO 缓存、派生缓存和异步编译；
- barrier 聚合和 GCL146；
- 96 批提交环、token/fence、Present 和延迟回收；
- XG tiling 和 CPU/GPU copy；
- 原生 RTV/DSV/SRV/UAV 创建。

这些属于资源与生命周期合同，而非单纯的性能快捷方式。

## 5. 建议的启用顺序

1. 先让 GCL put/limit 全零，保留其他对象和状态。
2. 强制 PSO 走 `SetPipelineState`，确认 shadow 存储有效。
3. 如需 descriptor 全原生，再单独启用 native CBV。
4. 为关键 XS 方法加计数/日志，确认实际调用的是 `SetPipelineState`、root setter 和 `DrawIndexedInstanced`。
5. 对比同一帧的资源、命令调用和画面，再逐个恢复原生内联路径。
6. 每次只恢复一类快路径，先 PSO，再 IA/root，最后 Draw 和私有 opcode 优化。

## 6. 验证标准

降级成功的判据不是 put 一直为零，而是：

- 所有预期的 XS 慢速调用确实发生；
- 没有因为空 put 导致直接写入或访问冲突；
- PSO shadow 写入没有越界；
- 软件 CBV 按目标选择原生或软件路径，没有遗漏；
- token/fence、Present 和资源回收时序不变；
- 画面和资源内容与降级前一致。

如果某条路径仍直接写命令流，应对照 `FAST_PATHS.md` 的九类清单检查是否在创建或某次 `ResetX` 后漏掉了重新置零。
