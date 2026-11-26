# 四个上游，和它们跟这条线的关系（2026-11-26 15:00）

## 地图

| 上游 | 头 | 跟我们的共同祖先 | 我们没有的提交 |
|------|----|------------------|----------------|
| `Marvisak/VIctory`（源头） | `63bcca4` | `ffe310e` | **479**（342 个非 merge，146 文件，+56,322 / −3,134） |
| `fxceless/victory-fork` | `46bf535` | `be721d5` | 3（但树里缺它 `a26859f..be721d5` 那 86 个提交的内容） |
| `trustno1-coder/VIctory-NVIDIA` | `033be27` | 就是它自己 | 0 —— **现在的基底** |
| `Harveyweiii/victory-fork` `local/combined-tested` | `d36f965` | 就是它自己 | 0 —— **shader 层已合** |

`Marvisak` 是最早的那个兼容层，另外三个都从它长出来，然后各走各的：
`fxceless` 从 `ffe310e` 之后分，`NVIDIA` 又从 `fxceless` 的 `a26859f` 分，
`Harvey` 又从 `fxceless` 的 `f85101b` 分。

## 已经合进来的

- **NVIDIA**（`e20d2ef`）：整棵树都成了基底。relay、card steps、compose、
  渲染缩放和自家 upscaler、动态槽、脸的修复、NaN scrub、play-nvidia 那套开关。
- **Harvey 的 shader 层**（`c7c7b9f`）：wave64 的 paired form、mesh 与 pixel 的
  链接、idiom/gfx10/ir 那套。compute 被拒从 11 个降到 5 个，**三个
  `XboxBvhBuilder` 程序现在翻译得出来了**。

## 合不进来的两个，和为什么

### fxceless：试了两次，两次都碎

它只差 3 个提交（`d4a528b` 纹理上传专用线程、`d8986cd` frame log 拆分、
`46bf535` 日志）。但要把 `a26859f..be721d5` 那 86 个提交的内容真正拿进树里，
就得动 `victory-gpu`，而两边的 gpu 层是两套架构：

```
                 NVIDIA 这边              fxceless 这边
host.rs 的子线程  relay / queue           queue2 / land / heap_worker
纹理上传          card_steps              heap_worker::Batch（Into::Ahead）
帧日志            （无）                  framelog / fpsbox / cardmeter / stats
回读              back_own_view           backreads / texback
```

试的结果：

- **gpu 层取我们的**：`heap_worker.rs` 要 `draw::Into::Ahead`、
  `HostList::refresh_textures_ahead`、`HostHeap::publish`，全没有 → 5 个错误
- **gpu 层取它的**：它的 `host.rs` 要它自己的 `indirect::GatherSet`、
  `indirect::WindowCheck`、`capture::read_back_of`，全没有 → 116 个错误

中间没有落脚点：两边的 `host.rs` 都改了八千行，`draw.rs` 各自加了不相容的抽象。

### Marvisak：479 个提交，差得最远

它的修复都很有用 ——

```
842b0cb GPU: the depth bounds test reaches the host (#158)
219878a GPU: a null shader view has no alias source
f3578e3 Shaders: decode f16 inline float constants in whole-wave v_add_f16 simulation (#2493)
e815cef shader: look up last byte for straddling memory writes (#2484)
366a978 Shaders: guarded scalar writes kill stale SGPRs in dead_on_edge (#2468)
ea488a9 GPU: no typed unordered view in a format the host cannot store to (#2462)
136e39d GPU: a page whose alias cannot be guarded stays changed (#2455)
```

我核对过：**这七条在 NVIDIA 和 fxceless 里一条都没有**。两个 fork 都没有跟进
源头这 479 个提交。共同祖先在 `ffe310e`，之后两边各长了 479 / 552 个提交，
146 个文件、5.6 万行的差异。

## 结论

- 想要的东西（能跑、能画、HUD、白天场景）现在都在 `main` 上
- `fxceless` 的 86 个提交和 `Marvisak` 的 479 个提交，**与 NVIDIA 的 gpu 架构互为
  替代而非补充**，靠 `git merge` 到不了同一个树里
- 真要三合一，得在 NVIDIA 这棵树上把 fxceless / Marvisak 的每一项**重写**进去
  （比如把 `queue2`/`land` 的能力按 relay 的接口重新实现），不是合并
- Marvisak 那七条 shader/gpu 修复是单项移植最划算的一批：它们是局部修复，
  不牵扯架构
