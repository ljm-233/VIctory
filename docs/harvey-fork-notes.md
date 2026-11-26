# Harveyweiii/victory-fork，`local/combined-tested`（2026-11-26）

## 它是什么

从 fxceless 的 `f85101b`（不是最新的 `be721d5`）分出去，23 个提交。
作者 PiloteCode 那几条是这次的重点。

```
d36f965 Publish the tested combined renderer and water readback configuration
de20896 Mesh shaders: exclude edge flags from primitive vertex indices
7d5abb3 Avoid texture self-copies and preserve overlapping source regions
bc015ad Launch the tested texture queue build for manual play
9342845 Bound texture queue sorting and collect its counters in one scan
9ec168a Document before and after visuals for shader binding changes
316b390 Anchor the visible 1080p game window to the primary display origin
faf8ef5 Scope texture read and remap exclusion to overlapping address ranges
48ff6a7 Integrate fxceless #51 to disable blending on integer render targets
550054d Integrate fxceless #860 to localize upload waits after validated fill differences
a62fe84 Keep the combined game client at 1080p and mute its own Windows audio sessions
21126ef Verify that numeric extent comparisons use the full descriptor size
af8ac32 Integrate fxceless PR 854 while retaining local shallow-water zero tests
09224bf / bf32230 Merge branch 'review-141' / 'review-849'
991481f Exercise the parallel compare for every piece in the in-flight fetch test
8a155bd Integrate fxceless f85101b with ztrx parallel compare and local rendering fixes
1e9d94b Retain local geometry, shallow-water, and ztrx PR 93 integration
8b64afb GPU: wait before overwriting an in-flight texture staging buffer under VICTORY_LATE_WAIT/RELAY
8971052 Indirect test: the two groups charge the budget in either order
0751197 README, ray-tracing notes: the shader pack's numbers, the paired form, mesh programs linked with their pixel program (VICTORY_MESH_LINK)
26600c6 Shaders: wave64 programs in a paired form on 32-lane hosts, mesh programs over 32 KiB linked with their pixel program, and more: 230 more programs pass
19d40a7 Clippy: a sort by key, a bench's view kept but not read, a diagnostics-only count
```

## 最有价值的一条：`26600c6`

**wave64 程序在 32-lane host 上的 paired form**：每个 console wave 跑成一个
host wave，每个线程承担两个 lane（n 和 n + 32）；每条 per-lane 分支由整个 wave
走、EXEC 作为每个 lane 的 guard；lane 读、permute、lane mask 和 reduce 都通过
host wave 自己的操作同时处理两半。

排在其它所有形式之后尝试（只针对 host 拒绝的、或翻译不了的程序），
排在近似 per-half 形式之前。作者在 RTX 5090 上量到：

```
25,682 / 25,953 程序翻译、validate、拿到管线
（9376fd6 上是 25,452；compute 2,729 对 2,695，mesh 8,694 对 8,657，
 pixel 12,067 对 11,947，vertex 2,192 对 2,153）
```

靠 paired form 通过的是 28 个程序：屏幕空间追踪 tile、粒子、decal 拖尾、
头发阴影遮罩、alpha resolve、**三个结构构建程序**、一个级联阴影程序。
另有 36 个「每个半 wave 自己计数/OR/elect」的程序从近似变成精确。
开关 `VICTORY_WAVE_PAIRS=0`。

**那「三个结构构建程序」就是我们被迫 `VICTORY_BVH_MISS=1` 全 miss 掉的东西。**
如果能跑，光追的反弹光和灯影就有机会回来。

## 我们主线的实际数字（`vicitry translate --list`，26,177 个程序）

```
programs     26177 translated of 26177 (40688031 IR instructions)
I/O          14259 of 14259 vertex and pixel programs raised
DXIL         14259 emitted of 14259 vertex and pixel programs
              2752 emitted of 2763 compute programs
              8765 emitted of 8765 mesh programs
```

- 翻译：**26,177 / 26,177，一个不落**
- emit 失败：**11 个 compute**（2752 / 2763）
- 提示性原因（不是拒绝）：195 个 voxel GS 的 SGPR 地址、5 个 descriptor 只当数据读、
  **3 个「first-lane read where not every lane runs」（例子正是 `XboxBvhBuilder.dll`）**、
  3 个 lane mask

也就是说 paired form 在我们这棵树上最多再救那 11 个，其中 3 个是 BVH 构建。
author 的 25,953 总数比我们的 26,177 小，两边扫描范围和 build 不同，不能直接对差。

## 为什么没有直接合

`git merge harvey/local/combined-tested` → **34 个文件、308 块冲突**，
是上次合 fxceless（38 块）的八倍。头部：

```
68 host.rs    24 shadow.rs    24 descriptor/host.rs    18 draw.rs
16 lower.rs   16 xmem.rs      15 watch.rs              15 texture.rs
11 texback.rs 10 wave.rs      10 resource.rs            9 queue.rs
```

试过「gpu 层取 NVIDIA、shader 层取 Harvey」这条捷径：

```
error: unresolved imports victory_shader::emit::{VIEW_INTEGER, VIEW_SIGNED}
error: unresolved import  victory_shader::emit::USER_ROOT_ADDRESS
error: unresolved import  victory_shader::emit::MEMORY_READ_ONLY
error: cannot find function set_raw_views / raw_views / constant_marks
error: cannot find value SLOT_STRIDE_CONSTANT / USER_DISPATCH_THREADS_X
```

NVIDIA 的 gpu 层依赖 NVIDIA 在 shader 层加的一批符号，Harvey 的 shader 没有；
反过来 Harvey 的 gpu 层（texback、texture queue、relay 的 wait）也是 NVIDIA 没有的。
**两边的 shader 层和 gpu 层是成对改的，换一层不行。**

再看 `wave.rs` 本身：436 行差异，不是新增而是重写 —— Harvey 把 `Sym` 变成
`Pair { lo, hi }`，`simulate` 的返回值从 `Left` 换成 `HashMap<Value, Shape>`，
删掉 `single`/`lane_number`，加 `Pair::of`/`syms`/`add64`。所有吃 `Shape` 的地方
（`lower.rs` 那边 1863 行）都得跟着改。**局部移植这条路走不通。**

## 结论

要么整棵合（308 块，几小时且不保证成功），要么放弃。
它值钱的地方就一处：让那 3 个 BVH 构建程序跑起来，从而能关掉 `BVH_MISS` 和
`SKIP_PIPELINES`，把光追的反弹光和灯影找回来。这一处值不值得付 308 块，
要看画面能不能接受现在的「太阳照不到的地方是黑的」。
