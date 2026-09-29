# 落地页（landing/）

单文件静态页：`index.html`，无构建步骤、无外部依赖。

## 部署（Cloudflare Pages，免费子域）

**方式一（拖拽）**：Cloudflare Dashboard → Workers & Pages → Create → Pages → Upload assets，把本目录拖进去，项目名取 `slimit`，得到 `https://slimit.pages.dev`。

**方式二（wrangler CLI）**：

```sh
npx wrangler pages deploy landing --project-name slimit
```

## 域名（非必须）

免费 `*.pages.dev` 子域足够内测与软启动阶段使用；正式收费/投放前再选购域名（如 `slimit.app` / `slimit.cn`），在 Pages 项目 Custom domains 里绑定，几分钟生效。

## 发布前必改

- [ ] 下载按钮 `href="#"` → DMG 直链（公证完成后替换；未签名期链接到 BETA 说明页）
- [ ] footer 的「反馈问题」「更新日志」占位链接 → 实际 GitHub Issues / Releases 地址（仓库转公开后）
- [ ] 定价「规划中」字样按发布策略保留或移除
