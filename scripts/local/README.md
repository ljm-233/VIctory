# 本地联合测试版

从仓库根目录打开 PowerShell，使用仓库指定的 Rust 工具链编译 Windows 诊断版。启动脚本按文件名选择版本。

```powershell
cargo build --locked --release -p victory --features diagnostics
Copy-Item .\target\release\victory.exe .\scripts\local\victory-material-plane.exe
```

已有剧情检查点时，指定自己的游戏目录和存档资料目录。`StorySeed` 指向包含 `7952987A` 子目录的资料目录。先用 `-CheckOnly` 核对配置，再去掉该参数启动。脚本复制存档后运行，采用 Series X、1080p、静音以及同步提交设置。

```powershell
.\scripts\local\start-fixed.ps1 -MaterialFix -GameDirectory 'D:\Games\MyGame' -StorySeed 'D:\VictoryProfiles\MyProfile' -CheckOnly
```

这个分支包含 GPU 水图回写、目标内存重新映射保护、分规格回写计数、同一帧的材质分类同步，以及网格着色器顶点索引的边缘标志修复。船只浮力版在本机任务中验证过持续航行。合入网格改动后，验证项目包括带边缘标志的三角形像素输出和四种水图回写。最新组合版的同场景游玩效果还需实测。

水图回写移植自 [Geckomacabre/vx-layer 的 eee5667d](https://github.com/Geckomacabre/vx-layer/commit/eee5667d7119bc4a378214621e1b18a814ebe80f)。材质分类同步参考[同项目的 00633243](https://github.com/Geckomacabre/vx-layer/commit/00633243fa27bb399dad94f6027b413ad92b83e5)，网格索引修复取自 [952c4233](https://github.com/Geckomacabre/vx-layer/commit/952c42331a66272b0690a9d90d374ca054a62121)。
