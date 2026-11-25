# GDK / XS 文档交叉核对

本次核对使用 `_gdkdoc` 与 `xbox_docs_d3d12_xs`，权威顺序不变：当前 IDB 决定实际调用和布局，文档只确认名称、参数与语义。文档目录顺序不用于推断函数表槽位。

## 已解决：WaitFrameEventX flags=2

`_gdkdoc/en-us/docs/gdk/d3d12xbox_wait_frame_event_flags.html:44-72` 定义：

```cpp
D3D12XBOX_WAIT_FRAME_EVENT_FLAG_PROCESS_TOKEN_ON_TIMEOUT = 0x2;
```

它表示即使等待超时也处理该帧并生成 pipeline token。当前 EXE 在 `0x142482450` 使用：

```cpp
WaitFrameEventX(
    D3D12XBOX_FRAME_EVENT_ORIGIN,
    0, // 立即超时
    nullptr,
    D3D12XBOX_WAIT_FRAME_EVENT_FLAG_PROCESS_TOKEN_ON_TIMEOUT,
    &currentToken);
```

`_gdkdoc/en-us/docs/gdk/id3d12device_waitframeeventx.html:107-114` 说明可以提前开始构建帧，但提前 PresentX 会阻塞到真正的 frame origin。该文档同时解释了此 EXE 为什么以 timeout=0 获取 token，以及为什么 token 不等于 GPU 完成证明。

已同步更新：

- `pseudocode/02_frame_submission.cpp`：使用正式枚举名，不再写未命名 flags=2。
- `docs/UNRESOLVED.md`：将该项移出未决清单。
- `_d3d12xs_re/D3D12_XS_LAYOUT.md` 与 `scarlett_actual_layout.json`：证据升级为 B+H+D。

## Present 语义核对

### Plane 布局

`_gdkdoc/en-us/docs/gdk/d3d12xbox_present_plane_parameters.html:44-56` 的字段顺序与 EXE 构造顺序一致：Token、ResourceCount、ppResources、pSrcViewRects、pDestPlacementBase、ColorSpace、ScaleFilter、ExtendedDescCount、pExtendedDescs、Flags。IDB 另已直接证明 `sizeof=0x48`、`alignof=8`。

同文件 `:103-106` 明确说明 ScaleFilter 在 Scarlett 上被忽略。因此该字段属于 ABI，但不应期待它实际控制缩放过滤。

### ViewCount 和 null presentation

`_gdkdoc/en-us/docs/gdk/d3d12xbox_present_parameters.html:61-64` 说明 ViewCount=0 在有 plane 时按默认单视图解释，在无 plane 时表示零视图/null presentation。该语义支持将关闭路径 `PresentX(0,null,null)` 解释为 null-present/队列冲刷，而不是普通零帧画面。

### Present flags

`_gdkdoc/en-us/docs/gdk/d3d12xbox_present_flags.html:44-50` 给出：

- `NONE=0`
- `IMMEDIATE=1`
- `DO_NOT_WAIT=2`
- `PERSIST=4`
- `DISABLE_VARIABLE_REFRESH_RATE=8`

这与消费者在 `0x14248237C` 根据记录字节生成 0/1 flags 的路径吻合。该分支的 1 可解释为 `IMMEDIATE`。

`_gdkdoc/en-us/docs/gdk/d3d12xbox_present_plane_flags.html:44-47` 当前只公开 `NONE`，与 plane 零初始化相容。

## 本轮补充：Present 和 frame pacing 已闭环

当前 IDB 进一步证明：

- `PresentRecord` 在 `+0x08..+0x37` 内联最多两个 plane，消费者直接把记录内资源字段的地址用作 `ppResources`。
- 源矩形的 left/top 由零初始化，right/bottom 来自 view `+0x38/+0x3A` 的宽和高。
- 所有 plane 共用常量 `{0,0,1,1}`。`_gdkdoc/en-us/docs/gdk/d3d12xbox_view_rect.html:44-77` 确认这是归一化目标矩形。
- `D3D12XBOX_PRESENT_PARAMETERS` 只有 flags 非零分支；其余字段保持零。请求间隔为 0 时使用 `IMMEDIATE`，与上文 flags 文档一致。
- `0x14247AB10` 使用 60/120Hz 两组微秒表和 1、1、2、3、4 倍索引，调用 `SetFrameIntervalX` 后调用 `ScheduleFrameEventX(ORIGIN)`。自动倍率来自 `displayHz / estimatedFps`，限制在 1..4。
- 自定义目标间隔路径的 pipeline period 为 `max(2,ceil(target/length))`，origin offset 为 `length-target%length`（整除时为 0）。这与 `id3d12device_setframeintervalx.html:58-71` 和 `id3d12device_scheduleframeeventx.html:58-71` 的参数定义一致。

这些字段不再列为未决。`FrameSubmissionQueue` 的 packed state 也已由 `0x140006180/0x1400062D0` 证明是计数信号量，而非普通原子整数。

## 资源创建证据的范围

`_gdkdoc/en-us/docs/gdk/id3d12device8_createcommittedresource2.html:44-53` 和 `id3d12device8_createplacedresource1.html:44-52` 支持 Committed→Placed 的代际关系及 DESC1 演进，但没有给出 Device10/11 的数值槽位。

因此：

- 128=`CreatePlacedResource2` 仍是与文档相容的推导，不能升级为文档实测。
- 129=`CreateReservedResource2` 仍保持原证据等级。
- 130/131 的顺序仍不能由目录或语义确定。

## 仍未解决

- `CreateDeviceFlags=0x20`：`d3d12xbox_create_device_flags_xs.html:44-48` 只公开 NONE 和 ENABLE_AUTO_HDR，没有该位。
- `ReportGpuHaltX`：在线 topic 存在但正文受门禁；旧 `ReportGpuHangX` 是不同方法，不能用于确定 Halt 的 flags 或返回类型。
- `EnableLiveDebuggingX`：有 live-debugging 概念说明，但没有槽位证明。
- `CreateComponentPlacedResourceX2`、`AllocateFromCommandAllocatorX`、`OMSetFrontAndBackStencilRef`、`SetComputeShaderLaunchParametersX`：目录有名称，正文受 NDA 门禁。
- Device126：没有找到可以可靠对应的方法。

`xbox_docs_d3d12_xs/_INDEX.md:3-7` 明确记录：352 个导航 topic 中只有 8 个公开，344 个匿名访问返回 404。`gdk_new_topics.md` 只提供新增 topic 名称，不能当作函数原型或槽位证明。

## 结论

本次文档核对真正解决的是 `PROCESS_TOKEN_ON_TIMEOUT`，并补强了 Present、null presentation 和 immediate 的语义。其余新槽位继续保留原证据等级，不以“文档里看起来合理”代替当前 EXE 的直接证据。
