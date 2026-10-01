import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  applyPlan,
  explain,
  getSettings,
  listQuarantine,
  listRules,
  listSnapshots,
  purgeExpiredQuarantine,
  restoreItem,
  scanAndPlan,
  setSettings,
  volumeSummary,
  type AiSettings,
  type ApplyReport,
  type Explanation,
  type Manifest,
  type PlanItem,
  type RuleInfo,
  type ScanPlanResponse,
  type ScanProgress,
  type SnapshotInfo,
  type VolumeSummary,
} from "./bridge";

const GIB = 1024 ** 3;
const MIB = 1024 ** 2;
const fmtGiB = (b: number | null | undefined) =>
  b == null ? "—" : (b / GIB).toFixed(2) + " GiB";
const fmtSize = (b: number) =>
  b >= GIB ? (b / GIB).toFixed(2) + " GiB" : (b / MIB).toFixed(1) + " MiB";

const RISK_LABEL: Record<PlanItem["risk"], string> = {
  green: "🟢 低风险",
  yellow: "🟡 注意",
  red: "🔴 谨慎",
};

const SOURCE_LABEL: Record<string, string> = {
  rules: "规则库",
  cloud: "云端 AI",
  heuristic: "本地启发式",
};

type TaskStatus = "running" | "done" | "error";

interface ScanTask {
  id: number;
  path: string;
  status: TaskStatus;
  startedAt: number;
  filesDone: number;
  currentDir: string;
  result?: ScanPlanResponse;
  error?: string;
}

/** 当前秒级时钟（驱动"已用时"刷新）。 */
function useTick(active: boolean) {
  const [, setN] = useState(0);
  useEffect(() => {
    if (!active) return;
    const t = setInterval(() => setN((n) => n + 1), 1000);
    return () => clearInterval(t);
  }, [active]);
}

/**
 * 清理面板：多任务并发扫描（任务列表 + 实时进度）→ 选中任务查看计划 →
 * 勾选 → 两段式确认隔离执行 → 可恢复。
 * 红线：AI 解释仅为提示层；red/非 executable 项永不进入执行列表。
 */
function CleanPanel() {
  const [root, setRoot] = useState("");
  const [applying, setApplying] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [err, setErr] = useState("");
  const [tasks, setTasks] = useState<ScanTask[]>([]);
  const [selectedTaskId, setSelectedTaskId] = useState<number | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [reports, setReports] = useState<ApplyReport[] | null>(null);
  const [tips, setTips] = useState<Record<string, Explanation>>({});
  const [ruleTitles, setRuleTitles] = useState<Map<string, string>>(new Map());
  // 新手档：只展示 A 类（一次性大额）与 C 类（用户数据提示），隐藏会再生的
  // B 类缓存——「清完又长回来」是清理工具失去信任的主因。默认新手档。
  const [noviceMode, setNoviceMode] = useState(true);
  const seqRef = useRef(0);

  // 规则 id → 人类可读标题（进程内嵌数据，廉价；失败不影响主流程）。
  useEffect(() => {
    listRules()
      .then((rs) => setRuleTitles(new Map(rs.map((r) => [r.id, r.title]))))
      .catch(() => {});
  }, []);

  const anyRunning = tasks.some((t) => t.status === "running");
  useTick(anyRunning);

  const selectedTask = tasks.find((t) => t.id === selectedTaskId && t.status === "done") ?? null;

  useEffect(() => {
    const un = listen<ScanProgress>("scan-progress", (e) => {
      const p = e.payload;
      setTasks((ts) =>
        ts.map((t) =>
          t.id === p.seq
            ? { ...t, filesDone: p.files_done, currentDir: p.current_dir }
            : t,
        ),
      );
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  const startScan = () => {
    const path = root.trim();
    if (!path || tasks.some((t) => t.status === "running" && t.path === path)) return;
    const id = ++seqRef.current;
    setTasks((ts) => [
      { id, path, status: "running", startedAt: Date.now(), filesDone: 0, currentDir: path },
      ...ts,
    ]);
    setSelectedTaskId(id);
    setSelected(new Set());
    setReports(null);
    setTips({});
    setConfirming(false);
    scanAndPlan(path, 20)
      .then((result) => {
        setTasks((ts) =>
          ts.map((t) =>
            t.id === id
              ? {
                  ...t,
                  status: "done",
                  result,
                  filesDone: result.summary.file_count,
                  currentDir: "",
                }
              : t,
          ),
        );
        setSelected(
          new Set(
            result.plan
              .filter((p) => p.executable && !p.below_min_age)
              .map((p) => p.path),
          ),
        );
      })
      .catch((e) =>
        setTasks((ts) =>
          ts.map((t) => (t.id === id ? { ...t, status: "error", error: String(e) } : t)),
        ),
      );
  };

  const toggle = (path: string, executable: boolean) => {
    if (!executable) return;
    setSelected((s) => {
      const next = new Set(s);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };

  const runApply = async (confirmed: boolean) => {
    if (!selectedTask?.result || applying) return;
    const items = selectedTask.result.plan.filter(
      (p) => p.executable && selected.has(p.path),
    );
    if (items.length === 0) {
      setConfirming(false);
      return;
    }
    if (!confirmed) {
      setConfirming(true);
      return;
    }
    setConfirming(false);
    setApplying(true);
    setErr("");
    try {
      setReports(await applyPlan(items));
      setSelected(new Set());
    } catch (e) {
      setErr(String(e));
    } finally {
      setApplying(false);
    }
  };

  const loadTip = async (p: PlanItem) => {
    if (tips[p.path]) return;
    try {
      const e = await explain({
        path: p.path,
        actual_bytes: p.estimated_bytes,
        apparent_bytes: p.estimated_bytes,
        owner_bundle: null,
        nearest_rule_hits: [p.rule_id],
        age_days: p.age_days,
        below_min_age: p.below_min_age,
      });
      setTips((t) => ({ ...t, [p.path]: e }));
    } catch (err) {
      setErr(String(err));
    }
  };

  // 新手档隐藏 B 类（会再生的缓存类）；expert 为完整清单。
  const visiblePlan = selectedTask?.result
    ? noviceMode
      ? selectedTask.result.plan.filter(
          (p) => p.durability === undefined || p.durability !== "regenerating",
        )
      : selectedTask.result.plan
    : [];
  const hiddenB = (selectedTask?.result?.plan.length ?? 0) - visiblePlan.length;

  const planBytes = selectedTask?.result
    ? selectedTask.result.plan
        .filter((p) => p.executable && selected.has(p.path))
        .reduce((a, p) => a + p.estimated_bytes, 0)
    : 0;

  return (
    <section>
      <h2>清理</h2>
      <div className="row">
        <input
          value={root}
          onChange={(e) => setRoot(e.target.value)}
          placeholder="文件夹路径，支持 ~（如 ~/code 或 ~/Library/Caches）"
          aria-label="扫描路径"
        />
        <button onClick={startScan} disabled={!root.trim()}>
          开始扫描
        </button>
        <label className="hint mode-toggle">
          <input
            type="checkbox"
            checked={noviceMode}
            onChange={(e) => setNoviceMode(e.target.checked)}
          />
          新手档（只显示大额可回收与危险提示）
        </label>
      </div>
      {err && <p className="error">{err}</p>}
      {tasks.length > 0 && (
        <table className="tasks">
          <thead>
            <tr>
              <th>扫描任务</th>
              <th>状态 / 进度</th>
              <th>已用时</th>
            </tr>
          </thead>
          <tbody>
            {tasks.map((t) => {
              const elapsed = ((Date.now() - t.startedAt) / 1000).toFixed(0);
              const rate =
                t.status === "running" && elapsed !== "0"
                  ? ` · ${(t.filesDone / Number(elapsed) / 1000).toFixed(1)} 万条/秒`
                  : "";
              const dirName = t.currentDir.split("/").filter(Boolean).pop() ?? "";
              return (
                <tr
                  key={t.id}
                  className={t.id === selectedTaskId ? "task-selected" : ""}
                  onClick={() => t.status === "done" && setSelectedTaskId(t.id)}
                >
                  <td className="path">{t.path}</td>
                  <td>
                    {t.status === "running" && (
                      <span className="progress">
                        ⏳ {t.filesDone.toLocaleString()} 条 · 正在扫 {dirName}
                        {rate}
                      </span>
                    )}
                    {t.status === "done" && (
                      <span className="ok">
                        ✅ {t.result?.summary.file_count.toLocaleString()} 文件 ·
                        命中 {t.result?.plan.length} 项（点击查看）
                      </span>
                    )}
                    {t.status === "error" && <span className="error">❌ {t.error}</span>}
                  </td>
                  <td>{t.status === "running" ? `${elapsed}s` : "—"}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
      {selectedTask?.result && (
        <>
          <p className="totals">
            {selectedTask.result.summary.root} —{" "}
            {selectedTask.result.summary.file_count.toLocaleString()} 个文件 · 实际
            占用 {fmtSize(selectedTask.result.summary.actual)} · 规则命中{" "}
            {selectedTask.result.plan.length} 项
          </p>
          {selectedTask.result.plan.length === 0 && (
            <div className="empty">
              <b>未命中任何规则</b>
              扫描完成；当前规则库没有覆盖该路径下的目标
            </div>
          )}
          {noviceMode && hiddenB > 0 && (
            <p className="hint">
              已折叠 {hiddenB} 项「缓存类」目标（清完很快会再长回来，净收益低）。
              切换到专家档可查看全部。
            </p>
          )}
          {visiblePlan.length > 0 && (
            <>
              <table>
                <thead>
                  <tr>
                    <th></th>
                    <th>风险</th>
                    <th>可回收</th>
                    <th>规则</th>
                    <th>路径</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  {[...visiblePlan]
                    .sort(
                      (a, b) =>
                        Number(a.below_min_age) - Number(b.below_min_age) ||
                        b.estimated_bytes - a.estimated_bytes,
                    )
                    .map((p) => (
                      <PlanRow
                        key={p.rule_id + p.path}
                        item={p}
                        checked={selected.has(p.path)}
                        tip={tips[p.path]}
                        ruleTitle={ruleTitles.get(p.rule_id)}
                        onToggle={() => toggle(p.path, p.executable)}
                        onExplain={() => loadTip(p)}
                      />
                    ))}
                </tbody>
              </table>
              <div className="row">
                <button
                  onClick={() => runApply(confirming)}
                  disabled={applying || planBytes === 0}
                >
                  {applying
                    ? "执行中…"
                    : confirming
                      ? `✅ 确认执行（隔离 ${fmtSize(planBytes)}）`
                      : `执行（隔离 ${planBytes ? fmtSize(planBytes) : "0 MiB"}）`}
                </button>
                {confirming && (
                  <button className="ghost" onClick={() => setConfirming(false)}>
                    取消
                  </button>
                )}
                <span className="hint">
                  仅 green/yellow 可执行项；全部迁入隔离区，可随时恢复
                </span>
              </div>
            </>
          )}
        </>
      )}
      {reports && <ApplyReports reports={reports} />}
    </section>
  );
}

function PlanRow(props: {
  item: PlanItem;
  checked: boolean;
  tip: Explanation | undefined;
  ruleTitle: string | undefined;
  onToggle: () => void;
  onExplain: () => void;
}) {
  const { item: p, checked, tip, ruleTitle, onToggle, onExplain } = props;
  return (
    <>
      <tr>
        <td>
          <input
            type="checkbox"
            checked={checked}
            disabled={!p.executable}
            onChange={onToggle}
            aria-label={`选择 ${p.path}`}
          />
        </td>
        <td>
          <span className={`risk-badge risk-${p.risk}`}>{RISK_LABEL[p.risk]}</span>
          {p.age_days != null && (
            <span
              className={`age-badge${p.below_min_age ? " age-guarded" : ""}`}
              title={
                p.below_min_age
                  ? "最近仍有活动，暂不建议清理；等项目稳定一段时间后再看"
                  : "长时间未动，回收后重新构建的成本有限"
              }
            >
              {p.age_days} 天未动
            </span>
          )}
          {p.below_min_age && (
            <span className="hint"> 最近仍在使用，暂不提供清理</span>
          )}
        </td>
        <td>{fmtSize(p.estimated_bytes)}</td>
        <td className="path" title={p.rule_id}>
          {ruleTitle ?? p.rule_id}
        </td>
        <td className="path">{p.path}</td>
        <td>
          <button className="ghost" onClick={onExplain} disabled={!!tip}>
            {tip ? SOURCE_LABEL[tip.source] ?? "已解释" : "解释"}
          </button>
        </td>
      </tr>
      {tip && (
        <tr>
          <td colSpan={6} className="tip">
            {p.age_days != null && (
              <>
                <b>闲置时长：</b>
                {p.age_days} 天未动（
                {p.below_min_age
                  ? "仍在活跃使用，建议暂不清理"
                  : "回收收益一次性，不会反复长回来"}
                ）
                <br />
              </>
            )}
            <b>这是什么：</b>
            {tip.what}
            <br />
            <b>产生者：</b>
            {tip.producer}（置信度 {(tip.confidence * 100).toFixed(0)}% · 来源{" "}
            {SOURCE_LABEL[tip.source] ?? tip.source}）
            <br />
            <b>删除后果：</b>
            {tip.consequence}
          </td>
        </tr>
      )}
    </>
  );
}

function ApplyReports({ reports }: { reports: ApplyReport[] }) {
  const ok = reports.filter((r) => r.error === null);
  const bad = reports.filter((r) => r.error !== null);
  return (
    <div className="reports">
      <p className="totals">
        ✅ 已隔离 {ok.length} 项
        {bad.length > 0 && <> · ❌ 失败 {bad.length} 项</>}
      </p>
      {bad.map((r) => (
        <p key={r.item.path} className="error">
          {r.item.path}: {r.error}
        </p>
      ))}
    </div>
  );
}

function AiSettingsPanel() {
  const [s, setS] = useState<AiSettings | null>(null);
  const [msg, setMsg] = useState("");

  useEffect(() => {
    getSettings()
      .then(setS)
      .catch((e) => setMsg(String(e)));
  }, []);

  if (!s) return null;
  const save = async () => {
    setMsg("");
    try {
      await setSettings(s);
      setMsg("已保存");
    } catch (e) {
      setMsg(String(e));
    }
  };

  return (
    <section>
      <h2>AI 设置</h2>
      <p className="hint">
        默认全部离线（规则库 + 本地启发式）。开启云端后，解释请求中的路径与大小会
        发往你配置的端点；执行授权仍然只来自规则库，AI 永无删除权。
      </p>
      <div className="row">
        <label>
          <input
            type="checkbox"
            checked={s.enabled}
            onChange={(e) => setS({ ...s, enabled: e.target.checked })}
          />{" "}
          启用云端 AI 解释（OpenAI 兼容接口，使用你自己的 Key）
        </label>
      </div>
      {s.enabled && (
        <>
          <div className="row">
            <input
              value={s.base_url}
              onChange={(e) => setS({ ...s, base_url: e.target.value })}
              placeholder="Base URL，如 https://open.bigmodel.cn/api/paas/v4"
            />
            <input
              value={s.model}
              onChange={(e) => setS({ ...s, model: e.target.value })}
              placeholder="模型名，如 glm-4-flash"
              style={{ maxWidth: 200 }}
            />
          </div>
          <div className="row">
            <input
              type="password"
              value={s.api_key}
              onChange={(e) => setS({ ...s, api_key: e.target.value })}
              placeholder="API Key（仅存本机 config.json）"
            />
            <button onClick={save}>保存</button>
            {msg && <span className="hint">{msg}</span>}
          </div>
        </>
      )}
      {!s.enabled && (
        <div className="row">
          <button onClick={save}>保存</button>
          {msg && <span className="hint">{msg}</span>}
        </div>
      )}
    </section>
  );
}

function QuarantinePanel() {
  const [items, setItems] = useState<Manifest[] | null>(null);
  const [err, setErr] = useState("");
  const [restored, setRestored] = useState<string | null>(null);
  const [purgeConfirming, setPurgeConfirming] = useState(false);
  const [purging, setPurging] = useState(false);
  const [purged, setPurged] = useState<Manifest[] | null>(null);

  const run = async () => {
    setErr("");
    try {
      setItems(await listQuarantine());
    } catch (e) {
      setErr(String(e));
    }
  };

  // 面板随 tab 激活挂载，进入即自动加载（保留刷新按钮手动重取）。
  useEffect(() => {
    run();
  }, []);

  // 面板随 tab 激活挂载，进入即自动加载（保留刷新按钮手动重取）。
  useEffect(() => {
    run();
  }, []);

  const restore = async (id: string) => {
    setErr("");
    try {
      setRestored(await restoreItem(id));
      await run();
    } catch (e) {
      setErr(String(e));
    }
  };

  // 两段式确认对齐 CleanPanel：第一次点击进入确认态，第二次才真正执行。
  // 不可逆删除（红线④）：执行后经后端审计日志记录，UI 提示被清理项。
  const purge = async () => {
    setErr("");
    setPurging(true);
    try {
      const cleaned = await purgeExpiredQuarantine();
      setPurged(cleaned);
      setPurgeConfirming(false);
      await run();
    } catch (e) {
      setErr(String(e));
    } finally {
      setPurging(false);
    }
  };

  return (
    <section>
      <h2>隔离区</h2>
      <div className="row">
        <button onClick={run}>刷新</button>
        <button
          onClick={() => (purgeConfirming ? purge() : setPurgeConfirming(true))}
          disabled={purging}
        >
          {purging
            ? "清理中…"
            : purgeConfirming
              ? "✅ 确认清理已过期条目（14 天，不可恢复）"
              : "清理已过期条目（14 天）"}
        </button>
        {purgeConfirming && (
          <button className="ghost" onClick={() => setPurgeConfirming(false)}>
            取消
          </button>
        )}
      </div>
      {err && <p className="error">{err}</p>}
      {restored && <p className="ok">已恢复到：{restored}</p>}
      {purged && (
        <p className="ok">
          已清理 {purged.length} 个过期条目（payload + manifest 一并移除，审计已记录）
        </p>
      )}
      {items && items.length === 0 && (
        <div className="empty">
          <b>隔离区为空</b>
          执行清理后条目会迁入此处，14 天内可随时恢复
        </div>
      )}
      {items && items.length > 0 && (
        <>
          <p>{items.length} 个条目</p>
          <table>
            <thead>
              <tr>
                <th>原路径</th>
                <th>大小</th>
                <th>规则</th>
                <th>隔离时间</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {items.map((m) => (
                <tr key={m.id}>
                  <td className="path">{m.original_path}</td>
                  <td>{fmtSize(m.actual_bytes)}</td>
                  <td className="path">{m.rule_id}</td>
                  <td>{m.quarantined_at}</td>
                  <td>
                    <button className="ghost" onClick={() => restore(m.id)}>
                      恢复
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
    </section>
  );
}

function VolumePanel() {
  const [mount, setMount] = useState("/System/Volumes/Data");
  const [err, setErr] = useState("");
  const [info, setInfo] = useState<VolumeSummary | null>(null);

  const run = async () => {
    setErr("");
    try {
      setInfo(await volumeSummary(mount.trim()));
    } catch (e) {
      setErr(String(e));
    }
  };

  return (
    <section>
      <h2>卷容量</h2>
      <div className="row">
        <input value={mount} onChange={(e) => setMount(e.target.value)} />
        <button onClick={run}>查询</button>
      </div>
      {err && <p className="error">{err}</p>}
      {info && (
        <ul>
          <li>
            卷：{info.volume_name ?? "?"}（{info.device_identifier ?? "?"}）
          </li>
          <li>文件系统剩余：{fmtGiB(info.free_space)}</li>
          <li>
            APFS 容器：{fmtGiB(info.apfs_container_size)}，剩余{" "}
            {fmtGiB(info.apfs_container_free)}
          </li>
          {info.system_snapshot_name && (
            <li>系统密封快照：{info.system_snapshot_name}</li>
          )}
        </ul>
      )}
    </section>
  );
}

function RulesPanel() {
  const [rules, setRules] = useState<RuleInfo[] | null>(null);
  const [err, setErr] = useState("");
  const [os, setOs] = useState<"all" | "macos" | "linux" | "windows">("all");
  const [risk, setRisk] = useState<"all" | "green" | "yellow" | "red">("all");
  const [q, setQ] = useState("");

  const run = async () => {
    setErr("");
    try {
      setRules(await listRules());
    } catch (e) {
      setErr(String(e));
    }
  };

  // 面板随 tab 激活挂载，进入即自动加载（规则库为进程内嵌数据，廉价）。
  useEffect(() => {
    run();
  }, []);

  // 面板随 tab 激活挂载，进入即自动加载（规则库为进程内嵌数据，廉价）。
  useEffect(() => {
    run();
  }, []);

  const filtered = (rules ?? []).filter((r) => {
    if (os !== "all" && r.os !== os) return false;
    if (risk !== "all" && r.risk !== risk) return false;
    if (q) {
      const needle = q.toLowerCase();
      if (
        !r.id.toLowerCase().includes(needle) &&
        !r.title.toLowerCase().includes(needle) &&
        !r.paths.some((p) => p.toLowerCase().includes(needle))
      )
        return false;
    }
    return true;
  });

  const byOs = (rules ?? []).reduce(
    (acc, r) => {
      acc[r.os] = (acc[r.os] || 0) + 1;
      return acc;
    },
    { macos: 0, linux: 0, windows: 0 } as Record<string, number>,
  );

  return (
    <section>
      <h2>规则库（{rules ? rules.length : "?"} 条）</h2>
      <div className="row">
        <button onClick={run}>刷新</button>
        {rules && (
          <>
            <select value={os} onChange={(e) => setOs(e.target.value as any)}>
              <option value="all">全部平台</option>
              <option value="macos">macOS ({byOs.macos})</option>
              <option value="linux">Linux ({byOs.linux})</option>
              <option value="windows">Windows ({byOs.windows})</option>
            </select>
            <select value={risk} onChange={(e) => setRisk(e.target.value as any)}>
              <option value="all">全部风险</option>
              <option value="green">🟢 低风险</option>
              <option value="yellow">🟡 注意</option>
              <option value="red">🔴 谨慎</option>
            </select>
            <input
              placeholder="搜索 id / 标题 / 路径…"
              value={q}
              onChange={(e) => setQ(e.target.value)}
            />
          </>
        )}
      </div>
      {err && <p className="error">{err}</p>}
      {rules && (
        <>
          <p className="hint">
            共 {filtered.length} 条规则（{rules.length} 条总库）。规则库是可信输入，
            任何失败即整体报错；UI 仅展示，不触发执行。
          </p>
          {filtered.length === 0 && (
            <div className="empty">
              <b>无匹配规则</b>
              试试放宽平台 / 风险筛选，或清空搜索词
            </div>
          )}
          <div className="rules-list">
            {filtered.map((r) => (
              <details key={r.id} className="rule-card">
                <summary>
                  <span className={`risk-badge risk-${r.risk}`}>
                    {RISK_LABEL[r.risk]}
                  </span>
                  <span className="rule-id">{r.id}</span>
                  <span className="rule-os">{r.os}</span>
                </summary>
                <p className="rule-title">{r.title}</p>
                <p className="rule-what">{r.what}</p>
                <p>
                  <strong>产生方：</strong>
                  {r.producer}
                </p>
                <p>
                  <strong>删除后果：</strong>
                  {r.consequence}
                </p>
                <p>
                  <strong>安全原因：</strong>
                  {r.safe_to_delete_because}
                </p>
                <p>
                  <strong>自动重建：</strong>
                  {r.regenerate}
                </p>
                <p>
                  <strong>典型大小：</strong>
                  {r.typical_size} · 恢复：{r.recovery}
                </p>
                <p>
                  <strong>路径：</strong>
                </p>
                <ul className="rule-paths">
                  {r.paths.map((p) => (
                    <li key={p} className="path">
                      {p}
                    </li>
                  ))}
                </ul>
                {r.refs.length > 0 && (
                  <p>
                    <strong>参考：</strong>
                  </p>
                )}
                <ul className="rule-refs">
                  {r.refs.map((ref) => (
                    <li key={ref}>
                      <a href={ref} target="_blank" rel="noopener noreferrer">
                        {ref}
                      </a>
                    </li>
                  ))}
                </ul>
              </details>
            ))}
          </div>
        </>
      )}
    </section>
  );
}

function SnapshotsPanel() {
  const [volume, setVolume] = useState("/System/Volumes/Data");
  const [err, setErr] = useState("");
  const [snaps, setSnaps] = useState<SnapshotInfo[] | null>(null);

  const run = async () => {
    setErr("");
    try {
      setSnaps(await listSnapshots(volume.trim()));
    } catch (e) {
      setErr(String(e));
    }
  };

  return (
    <section>
      <h2>APFS 快照</h2>
      <div className="row">
        <input value={volume} onChange={(e) => setVolume(e.target.value)} />
        <button onClick={run}>列出</button>
      </div>
      {err && <p className="error">{err}</p>}
      {snaps && (
        <>
          <p>{snaps.length} 个快照</p>
          <table>
            <thead>
              <tr>
                <th>name</th>
                <th>purgeable</th>
                <th>limiting-shrink</th>
                <th>xid</th>
              </tr>
            </thead>
            <tbody>
              {snaps.map((s) => (
                <tr key={s.uuid}>
                  <td className="path">{s.name}</td>
                  <td>{s.purgeable ? "是" : "否"}</td>
                  <td>{s.limiting_container_shrink ? "是" : "否"}</td>
                  <td>{s.xid}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
    </section>
  );
}

/** Tab 分组：6 面板收进 4 个 tab，消除面板墙。 */
const TABS = [
  { id: "clean", label: "清理" },
  { id: "quarantine", label: "隔离区" },
  { id: "rules", label: "规则库" },
  { id: "advanced", label: "高级" },
] as const;

type TabId = (typeof TABS)[number]["id"];

export default function App() {
  const [tab, setTab] = useState<TabId>("clean");
  return (
    <main>
      <h1>SlimIt</h1>
      <p className="hint">
        扫描 → 规则计划 → 隔离执行 → 可恢复；AI 解释仅作提示，永不影响执行
      </p>
      <nav className="tabs" aria-label="功能面板">
        {TABS.map((t) => (
          <button
            key={t.id}
            aria-selected={tab === t.id}
            className={tab === t.id ? "active" : ""}
            onClick={() => setTab(t.id)}
          >
            {t.label}
          </button>
        ))}
      </nav>
      {tab === "clean" && <CleanPanel />}
      {tab === "quarantine" && <QuarantinePanel />}
      {tab === "rules" && <RulesPanel />}
      {tab === "advanced" && (
        <>
          <VolumePanel />
          <SnapshotsPanel />
          <AiSettingsPanel />
        </>
      )}
    </main>
  );
}
