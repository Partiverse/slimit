# Skill: slimit-rule-author

Author, review, and validate SlimIt storage rules (`rules/**/*.yaml`) in the SlimIt repo. Use whenever the user asks to add/edit/批量生成/审查 a cleanup rule or rule library entry — 清理目标、规则、path entry, "把这个目录加进规则库", batch rule generation for macOS/Windows/Linux.

## Rule library contract (read before writing)

- Repo root: the SlimIt workspace containing `rules/`.
- Schema: `rules/schema-v1.json` (`slimit.rules/v1`) is the only contract. Schema changes require a version bump and PR.
- Authoring guide: `rules/README.md`. Template: `rules/macos/_template.yaml`.
- One rule per file. Filename = rule id. Files prefixed with `_` are excluded from validation and packaging.

## Workflow

1. **Verify the fact first.** Every rule must rest on an official doc or authoritative source (vendor docs, man page, official CLI help). Search the web when unsure. A rule without a source is a hazard, not a shortcut. Record the URL in `refs`.
2. **Copy the template** for the target OS (`rules/macos/`, `rules/windows/`, `rules/linux/`) and fill every field. Write `semantics.*` copy in the target user's language: what it is, who produced it, what happens if deleted.
3. **Apply the risk discipline** (this is the product's core promise — misjudging risk destroys the brand):
   - `green`: contents regenerate automatically, no user-perceivable loss (caches, build artifacts, package-manager download caches).
   - `yellow`: deletable but with a cost or requires user judgment (old backups, snapshots, old kernels, update caches).
   - `red`: irreversible or high-risk. Must use `action.kind: advise` (or an `irreversible` command), and `red_flags` is required. When in doubt between two levels, choose the stricter one.
4. **Action constraints**:
   - `command` rules must always ship a `dry_run` command that previews the effect. A command without a preview never enters the library.
   - Deletion actions default to `delete_contents_only: true` — keep the directory itself.
   - Prefer the platform's official cleanup command over raw file deletion (`brew cleanup`, `journalctl --vacuum`, `DISM /StartComponentCleanup`, …). Rules may not invent destructive commands.
5. **Validate locally** before committing:

   ```bash
   python3 <skill-dir>/scripts/validate.py rules/
   ```

   CI runs the same check: duplicate ids, `red` missing `red_flags`, `command` missing `dry_run`, schema violations, empty `refs`.
6. **Batch generation**: for bulk rule creation, keep every batch through step 5 plus a human review pass on every `yellow`/`red` call. Never auto-merge risk upgrades.

## Hard requirements checklist

- [ ] `id` unique, kebab-case, platform prefix matches `os` (`macos-`/`win-`/`linux-`)
- [ ] every `paths` entry is a real, current path on the target OS (verify version drift — e.g. WeChat 3.9+ moved to `xwechat_files`)
- [ ] `red` → `red_flags` filled; `command` → `dry_run` present; `refs` non-empty
- [ ] no rule may delete: user documents/photos, SSH keys, chat data (WeChat/QQ), VM disks (`*.vhdx`, `Docker.raw`), package stores (`conda pkgs`), Xcode Archives, WinSxS contents
- [ ] `estimate.recovery` honest: `none` for anything irreversible

## Anti-example (rejected rule)

```yaml
# BAD: raw rm on a VM disk, no dry-run, wrong risk
id: win-docker-vhdx
risk: green              # WRONG: may contain volume data
action:
  kind: purge-dir        # WRONG: deleting ext4.vhdx destroys all images/volumes
paths: ["%LOCALAPPDATA%\\Docker\\wsl"]
```

Correct handling: `risk: yellow`, `action.kind: command` with `docker system prune` + dry_run, plus an `advise` note that vhdx compaction goes through diskpart/wsl --manage.
