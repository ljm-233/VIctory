
## 搬过来了（`ec3898c`）

判定：这个分支的 `hooks.rs` 已经完整覆盖我们自己的两个补丁 —— `WATCHDOG` 的两处
rva（`0x2215c27`、`0x23cf683`）和我们的逐字节相同，`OWN_UPSCALER` 用的 `0x7e9426`
（`0F 45 C1`）比我们的 `0x7e_93f0` 更准，还带 build hash 校验和可配置的
`VICTORY_RENDER_SCALE`。**我们的 `CodePatch` 和 watchdog hook 不需要搬。**

真正独有的只剩三个 STUB 开关（`VICTORY_EXPOSURE_STUB`、`AMBIENT_STUB`、
`SHADOWMASK_STUB`）和全部文档。STUB 搬不动：它们依赖我们侧的
`emit::translate_code` 和 `lower::STORE_PLAN`，而这个分支的 `lower.rs` 是另一套结构；
而且它自己的 `BVH_MISS` + `AMBIENT_AIR` + `SKIP_PIPELINES` 已经解决了同类问题
（pass 被拒后画面没有光）。所以没搬。

于是主线换了基底：

```
line/main             ec3898c  (nvidia/main 033be27 + 我们的两份 docs)
line/nvidia           ec3898c  (同一提交)
line/merged-fxceless  75c5a0b  (刚才合完 fxceless 的那版，留档)
nvidia/main           033be27
```

`line/main` 从 `75c5a0b` 换成 `ec3898c`，旧的那个留在 `line/merged-fxceless`。

### 跑起来

同一个 `play-nvidia.ps1` 的 speed 集，`--console series-s`，`/tmp/run-main-nv.sh`。
构建 0 错误，`victory.exe` 73 MB，栈 patch 16 MiB。

- **0 崩溃**，draws 峰值 6,144
- **画面到了至今最完整的一步**（`docs/local/newline-car-hud.png`）：
  红色跑车的车身高光和车门缝、后视镜、车窗反射、车里的两个人（**脸有五官**）、
  棕榈树剪影、路边店招的绿色发光和路灯的光晕、黄昏的蓝天空；
  **HUD 第一次正常** —— 左下角「毕福 / 格蕾塔」、小地图（带 N 指北针）、顶部 W/N/E 罗盘
- 帧率 0 fps，`swap 12 GiB`，内存 14/14 —— 和 `fork-nvidia` 那次一模一样，
  证明这不是构建的问题，就是内存

### 一句话

功能全在、画面最好、HUD 正常，卡在 14 GB 物理内存上。这台机器要真跑到
10 fps 以上，得先有 32 GB 内存。
