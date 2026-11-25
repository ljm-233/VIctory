# VIctory-Belobug-AMD 模拟器交叉核对

参考项目：`C:\Users\Administrator\Desktop\WorkSpace\github\VIctory-Belobug-AMD`

该项目包含同一 title 的 D3D12/XS 模拟实现和测试，可作为高价值辅助证据，但仍不覆盖当前 IDB。只有与当前 EXE 的调用、字段或结构相互印证的项目才更新证据。

## 1. 增强 Barrier：新增直接结构证据

模拟器 `crates/victory-gpu/src/list.rs:1842-1847` 将 GCL146 描述为增强 barriers：16 字节 group，并在 `:1976-1985` 标出 slot146 和 slot148。

当前 EXE 提供完全一致的构造证据：

- `0x14003210D`：`Barrier(groupCount, groups)`。
- `0x1400095E0`：构造 40 字节 buffer barrier，类型为 `D3D12_BARRIER_TYPE_BUFFER=2`。
- `0x1400097F0`：构造 64 字节 texture barrier，类型为 `D3D12_BARRIER_TYPE_TEXTURE=1`。
- `0x1400099F0`：构造含完整 mip/array/plane 子资源范围的 64 字节 texture barrier。

与公开 `D3D12_BARRIER_GROUP` 定义对齐：

```cpp
struct D3D12_BARRIER_GROUP {       // 16 bytes
    D3D12_BARRIER_TYPE Type;       // +0x00
    UINT32 NumBarriers;            // +0x04
    const void* pBarriers;         // +0x08，按 Type 解释
};

struct D3D12_BUFFER_BARRIER {      // 40 bytes
    D3D12_BARRIER_SYNC SyncBefore; // +0x00
    D3D12_BARRIER_SYNC SyncAfter;  // +0x04
    D3D12_BARRIER_ACCESS AccessBefore; // +0x08
    D3D12_BARRIER_ACCESS AccessAfter;  // +0x0C
    ID3D12Resource* pResource;     // +0x10
    UINT64 Offset;                 // +0x18
    UINT64 Size;                   // +0x20
};

struct D3D12_TEXTURE_BARRIER {     // 64 bytes
    D3D12_BARRIER_SYNC SyncBefore; // +0x00
    D3D12_BARRIER_SYNC SyncAfter;  // +0x04
    D3D12_BARRIER_ACCESS AccessBefore; // +0x08
    D3D12_BARRIER_ACCESS AccessAfter;  // +0x0C
    D3D12_BARRIER_LAYOUT LayoutBefore; // +0x10
    D3D12_BARRIER_LAYOUT LayoutAfter;  // +0x14
    ID3D12Resource* pResource;     // +0x18
    D3D12_BARRIER_SUBRESOURCE_RANGE Subresources; // +0x20，24 bytes
    D3D12_TEXTURE_BARRIER_FLAGS Flags; // +0x38
};
```

当前 EXE 的容量也为实测值：

- 组数组容量 257（`0x101`），组计数位于 `+111008`。
- texture barrier 容量 1025（`0x401`），计数位于 `+111016`。
- buffer barrier 容量 1025，计数位于 `+111020`。
- 达到容量时先调用 pending flush，再继续写入。

因此 `PendingBarrierSet::count` 是**组数，不是资源转换总数**，且 GCL146 不能继续声明成旧 `D3D12_RESOURCE_BARRIER*`。

## 2. AllocateFromCommandAllocatorX：用途得到佐证

模拟器 `list.rs:1424-1445` 实现 slot148 / `0x4A0` 的 `(size, alignment) -> pointer`，并明确记录：**GTA VI 的 BVH builder 把 instance descriptions 放在这里**。返回内存由 list 持有到 Reset，CPU/GPU 使用相同地址。

这与现有 `AllocateFromCommandAllocatorX` 原型和 BVH 子系统相互吻合，证据升级为 O+D+T。但当前 IDB 的外部 `XboxBvhBuilder` 调用指向导入 thunk，未定位到本 EXE 内部的直接 `call [gcl+0x4A0]`，所以仍不标为 B 实测。

## 3. GCL 私有布局再次吻合

模拟器 `list.rs:98-132` 给出：

- `put` 从 `+0x18` 开始。
- 后续共有五个 put/limit 相关 qword。
- `pipeline_shadow + 32 == 0x560`。
- title 的 state flush 在 `+0x544..+0x564` 复制 32 字节 pipeline 数据，并测试 `+0x548`。

当前 EXE 的 `0x140032520` 也在相同位置复制 32 字节并检查相关位，支持 `CommandListEncodingState` 的 write cursor、limit 和 pipeline-state 区域模型。

模拟器 `device.rs:103-118` 还说明 device `+0x18` 的 out-of-line 标志：非零时，原本内联的调用改走函数表。这与本 EXE 原生 CBV/软件描述字两种模式一致。

## 4. Queue 和提交布局吻合

模拟器 `queue.rs` 提供：

- `ExecuteCommandLists2X` 的形状 `(count, lists, frameToken, flags)`，见 `:175-178`。
- Present plane 以 frame token 开头，后接资源，见 `:168-174`。
- 新 Queue 的 Signal 为 `0x80`、PresentX 为 `0x100`、提交为 `0x118`，见 `:639-643`。
- 旧 Signal `0x70 → 0x80`、旧 Present `0xF0 → 0x100` 的平移测试，见 `:1038-1040`。

这些与当前 EXE 的 Queue16/32/35 完全一致。

## 5. Frame token 语义再次吻合

模拟器 `frame.rs:19-25` 明确定义 `TOKEN_ON_TIMEOUT=2`，并在 `:143-179` 实现：origin 等待超时仍产生新帧 token。其注释说明该 title 通过 origin poll 提前开始下一帧，再通过 blocking flip wait 控制 Present 节奏。

这与 GDK 的 `PROCESS_TOKEN_ON_TIMEOUT` 文档以及 EXE 的 `WaitFrameEventX(ORIGIN,0,flags=2)` 形成三方印证。

## 6. Device 和资源创建槽位吻合

模拟器 `device.rs:354-404` 明确给出：

- Device127 / `0x3F8` = `CreateCommittedResource3`。
- Device133 / `0x428` = placed-at-address resource。
- Device73 / `0x248` = `CreateCommandListX`。
- Device86 / `0x2B0` = pipeline stream。
- Device62 / `0x1F0` = compute pipeline X。

`resource.rs:994-1003` 还实际调用 PC `CreateCommittedResource3`，与当前原型中的 `D3D12_RESOURCE_DESC1` 和 barrier layout 演进一致。

## 7. 没有新增证明的项目

该项目没有为以下未决项提供可靠的新证据：

- Device126 的名称。
- Device128/129、130/131 的顺序。
- `CreateDeviceFlags=0x20` 的含义。
- `ReportGpuHaltX` 的 flags 和返回类型。
- GCL144/145、147。
- Present 的全部私有显示时序字段。

这些项目继续保持 `UNRESOLVED.md` 中的原证据等级，不因模拟器未实现或留空而推断为“保留槽位”。

## 8. 已同步修改

- `05_command_recording.cpp`：`PendingBarrierSet` 改为增强 group/buffer/texture 模型，加入容量和分组逻辑。
- `CLASS_INDEX.md`：说明组数、结构大小和容量。
- `D3D12_XS_LAYOUT.md`、`scarlett_actual_layout.json`：补充 GCL146 结构和 GCL148 的 BVH 使用证据。
- `UNRESOLVED.md`：保留 GCL147 等未决项，并说明 GCL148 尚无当前 EXE 直接调用点。
