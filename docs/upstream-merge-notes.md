# 跟进 fxceless/victory-fork 上游（2026-11-26）

## 上游动了什么

`origin/main` 从 `310c6eb` 走到 `be721d5`，85 个提交。要点：

| 提交 | 内容 |
|------|------|
| `96ff135` | `VICTORY_WAVE_WIDE=last`（fork PR #1141）——64 线程的 pass 在这块 32 线程卡上的路 |
| `f5c03f3` | `VICTORY_TEXTURE_BACK=1`（fork PR #999）：着色器写、CPU 读的纹理回拷进标题内存。**水面靠这个才对**，没有它船会沉、任务失败 |
| `a35c557` | 管线声明的目标格式与实际绑定不同时给一个变体（`VICTORY_TARGET_VARIANTS`） |
| `2095f15` | 画面左上角的 corner box：帧率、CPU、最忙线程、卡、卡内存、PC 内存 |
| `32be36d` | `VICTORY_LOOKUP_CACHE=1`：一帧约省 14 ms（run 293 实测） |
| `6aed078` | `VICTORY_CARD_SYNC_BATCH`：发+哈希的列表分三步走 |
| `895a23b` | `VICTORY_HEAP_PASS_MARKED` / `VICTORY_LIST_REUSE` |
| `988aa41` | 动态槽环 262,144 满导致 table 被拒的定量结论 |

关键澄清（`diagnostics/README.md` run 295、run 301）：

- **黑面不是曝光 bug**：`VICTORY_CARD_WINDOWS_MB=2048` 把卡占满，卡到 91% 时约 19,700
  个纹理排队等空间，G-buffer 基础色 37% 的屏幕是 0，房子、楼梯、地面就没纹理。
  降到 512 后 1.5 GB 让出来，木材、楼梯、地面立刻有纹理。
- **阴影区全黑是上游也没解决的**：光追的 bounce 光、灯和太阳的追踪阴影还没跑，
  太阳没直射到的地方就是黑。和我们看到的完全一致。
- 上游测试机：Windows 11 / i7-14700KF / RTX 4070 **12 GB** / **32 GB** 内存。
  成绩 7-12 fps，能玩过第一个船任务，30-50 分钟后卡丢失（0x887A0005）。

## 为什么没有整棵 merge

`638f22e`（作者 `main`）**不在** upstream 的历史里。`git merge-base line/main origin/main`
= `310c6eb`，我们这侧 219 个提交、上游 85 个提交，两边都从同一个点平行长出去。

`git merge origin/main` 出 17 个冲突、16 个文件、36 个冲突块，且是语义冲突：

- `resource.rs`：上游整块加了 `crate::tiles`（levels on demand），我们这侧把
  `under_share` 抽成函数、加了 `copies_cap` / `room_percent`
- 同类改写还有 `host.rs`（8 块）、`shadow.rs`、`texture.rs`、`watch.rs`、
  `shader_cache.rs`、`draw.rs`、`indirect.rs`、`descriptor/host.rs`、
  `pipeline/{host,library}.rs`、`console_insn.rs`、`xgamesave.rs`、`emit/lower.rs`、
  `build.rs`

结论：整棵 merge 不是半小时能干的事。改为**独立验证上游**，成立再换主线。

## 独立构建上游（worktree `fork-upstream`）

```
git worktree add /mnt/winbot-software/GTA6/fork-upstream --detach origin/main   # be721d5
```

本机编译上游要补四件事，缺一件就起不来：

1. **toolchain**：系统 `cargo` 是 Arch 1.100.0，没有 `x86_64-pc-windows-gnu` 的 std。
   用 rustup 的 `stable`（同样 1.100.0）——它的 `lib/rustlib/x86_64-pc-windows-gnu` 是全的。
   ```
   TC=$HOME/.rustup/toolchains/stable-x86_64-unknown-linux-gnu
   PATH="$MW:$TC/bin:$PATH" $TC/bin/cargo build --release --target x86_64-pc-windows-gnu -p victory
   ```
2. **dlltool**：`error calling dlltool 'x86_64-w64-mingw32-dlltool'` —— 把
   `/home/link/toolchain-dl/mingw/usr/bin` 放进 `PATH`。
3. **zlib-rs 0.6.8**：上游 `Cargo.lock` 要 0.6.8，vendor 里只有 0.6.7。
   从 `static.crates.io` 拉 `.crate` 解进 `vendor/`，写 `.cargo-checksum.json`。
4. **`d3dcompiler_47.dll`**：上游 exe 静态导入它，release 目录里没有就走 wine 内建那个，
   而 wine 内建的要 `wined3d.dll` → `libvkd3d-*`（本机没有），链式 `c0000135` 起不来。
   把 `victory-fork-ref/target/.../release/d3dcompiler_47.dll`（4.4 MB）复制过来即可。

另外照例把栈 patch 到 16 MiB（`pe+24+0x48`，2 MiB → 16 MiB）。

## 跑起来的结果

开关：`VICTORY_INDIRECT_WINDOW=1 VICTORY_TEXTURE_NO_WAIT=1 VICTORY_TEXTURE_BACK=1
VICTORY_LOOKUP_CACHE=1 VICTORY_CARD_WINDOWS_MB=512 VICTORY_CARD_BUFFERS_MIN_KB=64
VICTORY_CARD_BUFFERS_MB=1280 VICTORY_LEVELS_ON_DEMAND=1 VICTORY_COPY_PLAIN=1
VICTORY_JOIN_PARTS=1 VICTORY_BACK_OWN_VIEW=1`

- **画面正常**：Lucia 和一个男的在海边夜谈（字幕「认识，他……是我男朋友。」），
  天空、城市灯带、海面反光、前景的粉色车顶都在。剧情比 `line/main` 那版更远。
  截图 `docs/local/upstream-first-frame.png`
- **0 崩溃**（没有 `0xc0000005`，也没有 `unhandled exception`）
- **但只有 0.2 fps**，5649 ms 一帧，664 draws
- corner box 暴露了原因：
  ```
  0.2 FPS  5000 MS
  CPU 52%  GAME 3.2 CORES
  TOP THREAD 100%
  GPU NOT KNOWN
  VRAM 6.2/6.6 GB
  RAM 5.2 GB  FREE 0.8 GB
  ```
  `RAM FREE 0.8 GB`、系统 13/14 GB 已用 —— **内存是硬瓶颈**。上游自己报的运行占用是
  23-27 GB / 32 GB 内存、9.2-9.6 GB / 10.3 GB 卡。这台机器 **14 GB 内存、8 GB 卡**，
  差了整整一个档。

## 现在的判断

- 上游那 85 个提交的价值是**正确性**（水面、纹理、目标格式变体、黑面成因），不是速度；
  速度靠的是它那套开关，而那套开关是按 12 GB / 32 GB 的机器标定的。
- `line/main` 在这台机器上反而更快（0-1 fps，draws 3000-5000），因为它的默认更保守。
- 值得从上游拿过来的三样：`VICTORY_TEXTURE_BACK=1`（水面）、
  `VICTORY_CARD_WINDOWS_MB` 调小治黑面、`VICTORY_TARGET_VARIANTS` 的目标格式修复。
- 全黑阴影区两边一样，是光追没跑，不是曝光 stub 的锅。
