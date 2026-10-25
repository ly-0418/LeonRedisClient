# LeonRedis Windows 安装包

- 版本：v1.0.0
- 文件：`LeonRedis_1.0.0_x64-setup.exe`
- 平台：Windows x64
- 安装方式：NSIS
- 已存在任意版本时会静默卸载旧版本并覆盖安装，保留安装目录下的 `data` 配置。

校验：

```powershell
Get-FileHash .\LeonRedis_1.0.0_x64-setup.exe -Algorithm SHA256
```
