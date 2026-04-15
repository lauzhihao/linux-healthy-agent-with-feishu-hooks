# Security Policy

## 中文

请不要提交任何密钥型内容，包括：

- 飞书 webhook 真实地址
- `.env`
- private key
- SSH key
- AWS access key
- token
- password
- PEM 文件

飞书 webhook 应通过环境变量或 systemd `EnvironmentFile` 注入：

```bash
FEISHU_WEBHOOK_URL=https://open.feishu.cn/open-apis/bot/v2/hook/REPLACE_ME
```

请只提交占位符，不要提交真实值。

如果发现安全问题，请通过 GitHub Security Advisory 或私下联系维护者。

## English

Do not commit secret-like content, including:

- real Feishu webhook URLs
- `.env`
- private keys
- SSH keys
- AWS access keys
- tokens
- passwords
- PEM files

Inject Feishu webhooks through environment variables or systemd
`EnvironmentFile`:

```bash
FEISHU_WEBHOOK_URL=https://open.feishu.cn/open-apis/bot/v2/hook/REPLACE_ME
```

Commit placeholders only, never real values.

Please report security issues through GitHub Security Advisory or by contacting
the maintainer privately.
