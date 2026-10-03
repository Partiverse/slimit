import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  applyPlan,
  explain,
  getSettings,
  listQuarantine,
  listRules,
  listSnapshots,
  checkFda,
  openFdaSettings,
  probeManual,
  purgeQuarantineItem,
  purgeExpiredQuarantine,
  restoreItem,
  testAi,
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

  // 进度事件前端缓冲（与后端节流双保险，2026-10-02）：事件只更新 ref，
  // 定时器每 200ms 批量刷进 state——百万文件扫描不再逐事件重渲染冻屏。
  const progressRef = useRef(new Map<number, ScanProgress>());
  // 快扫（几秒内完成）时 200ms 定时器可能一次都没触发，缓冲数据永不进 state
  // ——用户所见「数字一直是 0」的最后一环。flushProgress 供定时器与扫描
  // 结束路径共用。
  const flushProgress = useRef(() => {});
  useEffect(() => {
    const flush = () => {
      if (progressRef.current.size === 0) return;
      const pending = progressRef.current;
      progressRef.current = new Map();
      setTasks((ts) =>
        ts.map((t) => {
          const p = pending.get(t.id);
          if (!p) return t;
          return { ...t, filesDone: p.files_done, currentDir: p.current_dir };
        }),
      );
    };
    flushProgress.current = flush;
    const un = listen<ScanProgress>("scan-progress", (e) => {
      progressRef.current.set(e.payload.seq, e.payload);
    });
    const timer = setInterval(flush, 200);
    return () => {
      un.then((f) => f());
      clearInterval(timer);
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
    scanAndPlan(path, 20, id)
      .then((result) => {
        // 扫描已结束：把缓冲里的最后一批进度刷进 state，否则快扫场景
        // 任务行永远停在 0 条。
        flushProgress.current();
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

  // 手动清理（2026-10-02）：用户自选条目。授权在用户点击；服务端
  // authorize 仍会重写自证字段并强制保护名单 + 隔离区。
  const [manualItems, setManualItems] = useState<PlanItem[]>([]);
  const [manualBusy, setManualBusy] = useState(false);

  const addManual = async (rawPath: string) => {
    const p = rawPath.trim();
    if (!p || manualBusy) return;
    setManualBusy(true);
    setErr("");
    try {
      const probe = await probeManual(p);
      if (manualItems.some((m) => m.path === probe.path)) {
        setErr("该路径已在计划中");
        return;
      }
      const item: PlanItem = {
        rule_id: "user-manual",
        path: probe.path,
        estimated_bytes: probe.actual_bytes,
        risk: "yellow",
        executable: true,
        age_days: null,
        below_min_age: false,
        durability: "one-shot",
        origin: "user-manual",
      };
      setManualItems((ms) => [...ms, item]);
      setSelected((s) => new Set(s).add(probe.path));
    } catch (e) {
      setErr(String(e));
    } finally {
      setManualBusy(false);
    }
  };

  const runApply = async (confirmed: boolean) => {
    if ((!selectedTask?.result && manualItems.length === 0) || applying) return;
    const ruleItems = (selectedTask?.result?.plan ?? []).filter(
      (p) => p.executable && selected.has(p.path) && viewSet.has(p.path),
    );
    const manual = manualItems.filter((m) => selected.has(m.path));
    const items = [...ruleItems, ...manual];
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
      setManualItems([]);
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
  // 大小阈值筛选（种子反馈：<1MB 的条目列表太噪）：仅影响展示与选择范围。
  const [minSize, setMinSize] = useState(0);
  // 年龄筛选（种子反馈）：只看闲置 ≥ N 天；无年龄证据的条目在筛选时不显示
  const [minAge, setMinAge] = useState(0);
  const sizeVisible = (selectedTask?.result?.plan ?? []).filter(
    (p) =>
      p.estimated_bytes >= minSize &&
      (minAge === 0 || (p.age_days != null && p.age_days >= minAge)),
  );
  const visiblePlan = noviceMode
    ? sizeVisible.filter(
        (p) => p.durability === undefined || p.durability !== "regenerating",
      )
    : sizeVisible;
  const hiddenB =
    sizeVisible.filter(
      (p) => p.durability !== undefined && p.durability === "regenerating",
    ).length +
    ((selectedTask?.result?.plan.length ?? 0) - sizeVisible.length);

  // 计划分页（种子反馈：条目多时执行键划太久）
  const [page, setPage] = useState(0);
  const PAGE_SIZE = 15;
  const paged = visiblePlan.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE);
  const pageCount = Math.max(1, Math.ceil(visiblePlan.length / PAGE_SIZE));
  useEffect(() => {
    if (page >= pageCount) setPage(0);
  }, [page, pageCount]);

  // 执行/统计只作用于当前筛选视图内的选中项（2026-10-02 逻辑错误专项）：
  // 被筛选掉的条目即使残留选中状态也不参与执行。
  const viewSet = new Set(visiblePlan.map((p) => p.path));
  const planBytes =
    (selectedTask?.result
      ? selectedTask.result.plan
          .filter((p) => p.executable && selected.has(p.path) && viewSet.has(p.path))
          .reduce((a, p) => a + p.estimated_bytes, 0)
      : 0) +
    manualItems
      .filter((m) => selected.has(m.path))
      .reduce((a, m) => a + m.estimated_bytes, 0);

  // FDA 授权状态（种子反馈：一个文件夹一个弹窗太累 + 授权后状态不更新）：
  // 挂载即检测，未授权显示常驻引导条；用户可关闭本会话提示。
  const [fdaGranted, setFdaGranted] = useState<boolean | null>(null);
  const [fdaDismissed, setFdaDismissed] = useState(false);
  useEffect(() => {
    checkFda()
      .then(setFdaGranted)
      .catch(() => setFdaGranted(null));
  }, []);
  const refreshFda = () => {
    setFdaGranted(null);
    checkFda()
      .then(setFdaGranted)
      .catch(() => setFdaGranted(null));
  };
  const runningTask = tasks.find((t) => t.status === "running");
  const runningDirName =
    runningTask?.currentDir.split("/").filter(Boolean).pop() ?? "";

  return (
    <section>
      <h2>清理</h2>
      {fdaGranted === false && !fdaDismissed && (
        <div className="fda-banner">
          ⚠️ 尚未开启「完全磁盘访问」：逐个文件夹授权很繁琐且扫不全。建议
          一次授权解决所有弹窗——
          <button className="ghost" onClick={refreshFda}>
            我已开启，重新检测
          </button>
          <button className="ghost" onClick={() => setFdaDismissed(true)}>
            本次忽略
          </button>
        </div>
      )}
      {fdaGranted === true && (
        <p className="hint">✅ 完全磁盘访问已授权（检测于本页加载时）</p>
      )}
      {runningTask && (
        <div className="scan-live" role="status">
          <span className="progress-bar" aria-hidden="true" />
          正在扫描 {runningTask.path} — 已发现{" "}
          {runningTask.filesDone.toLocaleString()} 项 · 正在扫 {runningDirName}
        </div>
      )}
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
        <label className="hint mode-toggle">
          只看 ≥{" "}
          <select
            value={minSize}
            onChange={(e) => setMinSize(Number(e.target.value))}
          >
            <option value={0}>全部</option>
            <option value={10485760}>10 MB</option>
            <option value={104857600}>100 MB</option>
            <option value={1073741824}>1 GB</option>
          </select>
        </label>
        <label className="hint mode-toggle">
          闲置 ≥{" "}
          <select
            value={minAge}
            onChange={(e) => setMinAge(Number(e.target.value))}
          >
            <option value={0}>不限</option>
            <option value={7}>7 天</option>
            <option value={30}>30 天</option>
            <option value={90}>90 天</option>
          </select>
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
              // 定宽速率：「0.0」补到「999.9」同宽（tabular-nums 等宽），
              // 配合 tasks 表 fixed 布局彻底消除扫描中列宽抖动。
              const rate =
                t.status === "running" && elapsed !== "0"
                  ? ` · ${(t.filesDone / Number(elapsed) / 1000)
                      .toFixed(1)
                      .padStart(5, " ")} 万条/秒`
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
                        <span className="progress-bar" aria-hidden="true" />
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
          {/* 手动清理（种子反馈「没命中规则就不能删吗」）：用户自选任意路径
              进隔离区。授权在用户；服务端仍强制保护名单 + 可恢复 + 审计。 */}
          <div className="row">
            <input
              placeholder="手动加入：粘贴任意文件/文件夹路径（支持 ~），如 ~/Downloads/xxx.dmg"
              onKeyDown={(e) => {
                if (e.key === "Enter") addManual((e.target as HTMLInputElement).value);
              }}
              id="manual-path-input"
            />
            <button
              className="ghost"
              disabled={manualBusy}
              onClick={() => {
                const el = document.getElementById("manual-path-input") as HTMLInputElement | null;
                if (el) {
                  addManual(el.value);
                  el.value = "";
                }
              }}
            >
              {manualBusy ? "探测中…" : "加入计划"}
            </button>
            <span className="hint">可恢复</span>
          </div>
          {manualItems.length > 0 && (
            <table>
              <thead>
                <tr>
                  <th></th>
                  <th>类型</th>
                  <th>可回收</th>
                  <th>路径</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {manualItems.map((m) => (
                  <tr key={m.path}>
                    <td>
                      <input
                        type="checkbox"
                        checked={selected.has(m.path)}
                        onChange={() => toggle(m.path, true)}
                      />
                    </td>
                    <td>
                      <span className="risk-badge risk-yellow manual-badge">手动</span>
                    </td>
                    <td>{fmtSize(m.estimated_bytes)}</td>
                    <td className="path">{m.path}</td>
                    <td>
                      <button
                        className="ghost"
                        onClick={() => {
                          setManualItems((ms) => ms.filter((x) => x.path !== m.path));
                          setSelected((s) => {
                            const n = new Set(s);
                            n.delete(m.path);
                            return n;
                          });
                        }}
                      >
                        移除
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          {/* 空间透镜 lite：大文件与大目录 Top 榜，一键加入手动计划。 */}
          {selectedTask.result.summary.top_files &&
            selectedTask.result.summary.top_files.length > 0 && (
              <details>
                <summary className="hint">
                  大文件 Top {selectedTask.result.summary.top_files.length}（安装包/影片等，可加入手动计划）
                </summary>
                <table>
                  <tbody>
                    {selectedTask.result.summary.top_files.map((f) => (
                      <tr key={f.path}>
                        <td className="path">{f.path}</td>
                        <td>{fmtSize(f.actual)}</td>
                        <td>
                          <button className="ghost" onClick={() => addManual(f.path)}>
                            加入计划
                          </button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </details>
            )}
          {selectedTask.result.summary.top_dirs.length > 1 && (
            <details>
              <summary className="hint">大目录 Top 10（先清哪里，可加入手动计划）</summary>
              <table>
                <tbody>
                  {selectedTask.result.summary.top_dirs
                    .filter((d) => d.path !== selectedTask.result!.summary.root)
                    .slice(0, 10)
                    .map((d) => (
                      <tr key={d.path}>
                        <td className="path">{d.path}</td>
                        <td>{fmtSize(d.actual)}</td>
                        <td>
                          <button className="ghost" onClick={() => addManual(d.path)}>
                            加入计划
                          </button>
                        </td>
                      </tr>
                    ))}
                </tbody>
              </table>
            </details>
          )}
          {selectedTask.result.plan.length === 0 && manualItems.length === 0 && (
            <div className="empty">
              <b>未命中任何规则</b>
              扫描完成；当前规则库没有覆盖该路径下的目标。也可以在上方手动粘贴路径加入计划
            </div>
          )}
          {noviceMode && hiddenB > 0 && (
            <p className="hint">
              已折叠 {hiddenB} 项「缓存类」目标（清完很快会再长回来，净收益低）。
              切换到专家档可查看全部。
            </p>
          )}
          {(visiblePlan.length > 0 || manualItems.length > 0) && (
            <>
              <div className="row">
                <button className="ghost" onClick={() => setSelected(new Set(visiblePlan.filter((p) => p.executable).map((p) => p.path)))}>
                  全选可执行
                </button>
                <button className="ghost" onClick={() => setSelected(new Set(visiblePlan.filter((p) => !selected.has(p.path)).map((p) => p.path)))}>
                  反选（仅当前视图）
                </button>
                <button className="ghost" onClick={() => setSelected(new Set())}>
                  全不选
                </button>
                <span className="hint">只作用于当前显示的条目</span>
              </div>
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
                  {[...paged]
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
              <div className="row sticky-actions">
                {pageCount > 1 && (
                  <span className="hint">
                    <button className="ghost" disabled={page === 0} onClick={() => setPage(page - 1)}>‹</button>
                    第 {page + 1} / {pageCount} 页（共 {visiblePlan.length} 条）
                    <button className="ghost" disabled={page >= pageCount - 1} onClick={() => setPage(page + 1)}>›</button>
                  </span>
                )}
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
        <td className="rule-cell" title={p.rule_id}>
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

function AdvancedPanel() {
  const [openErr, setOpenErr] = useState("");
  return (
    <section>
      <h2>高级</h2>
      <div className="fda-card">
        <b>权限引导：完全磁盘访问（推荐先做）</b>
        <p className="hint">
          扫描 ~/Library、代码目录等位置需要「完全磁盘访问」权限；没有授权时扫描
          也能运行，但很多目录会被系统挡住（结果偏小、部分条目看不到）。建议第一
          次使用时就授权，避免扫到一半再补权限。
        </p>
        <button
          onClick={async () => {
            try {
              await openFdaSettings();
            } catch (e) {
              setOpenErr(String(e));
            }
          }}
        >
          打开系统设置 → 完全磁盘访问
        </button>
        <p className="hint">
          在列表里勾选 Slimit（或「+」手动添加 /Applications/Slimit.app），然后
          重启 Slimit。扫描时若弹出权限请求，也请点允许。
        </p>
        {openErr && <p className="error">{openErr}</p>}
      </div>
      <AiSettingsPanel />
    </section>
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
            <button
              className="ghost"
              onClick={async () => {
                setMsg("测试中…");
                try {
                  setMsg(await testAi());
                } catch (e) {
                  setMsg(`❌ ${e}`);
                }
              }}
            >
              测试连接
            </button>
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

function QuarantinePanel({ active }: { active: boolean }) {
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

  // tab 激活即自动刷新（种子反馈「隔离后要手动刷新才出现」）；
  // 执行清理后切过来也必然最新。
  useEffect(() => {
    if (active) run();
  }, [active]);

  const restore = async (id: string) => {
    setErr("");
    try {
      setRestored(await restoreItem(id));
      await run();
    } catch (e) {
      setErr(String(e));
    }
  };

  // 单条立即删除（种子反馈「隔离区不能清理删除文件夹？」）：两段式确认，
  // 不可逆，后端审计 purge-entry。
  const [deleteConfirmId, setDeleteConfirmId] = useState<string | null>(null);
  // 一键批量（种子反馈「删除/恢复太繁琐」）：restore-all 逐条恢复；
  // purge-all 逐条彻底删除（均两段式确认）。
  const [batchConfirm, setBatchConfirm] = useState<null | "restore-all" | "purge-all">(null);
  const [batchBusy, setBatchBusy] = useState(false);
  const runBatch = async () => {
    if (!batchConfirm || batchBusy || !items) return;
    setBatchBusy(true);
    setErr("");
    try {
      for (const m of items) {
        if (batchConfirm === "restore-all") await restoreItem(m.id);
        else await purgeQuarantineItem(m.id);
      }
      setBatchConfirm(null);
      await run();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBatchBusy(false);
    }
  };
  const purgeOne = async (id: string) => {
    setErr("");
    try {
      await purgeQuarantineItem(id);
      setDeleteConfirmId(null);
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
        {items && items.length > 0 && (
          <>
            <button
              onClick={() =>
                batchConfirm === "restore-all"
                  ? runBatch()
                  : setBatchConfirm("restore-all")
              }
              disabled={batchBusy}
            >
              {batchConfirm === "restore-all" ? "✅ 确认全部恢复" : `一键恢复全部（${items.length}）`}
            </button>
            <button
              className="danger"
              onClick={() =>
                batchConfirm === "purge-all" ? runBatch() : setBatchConfirm("purge-all")
              }
              disabled={batchBusy}
            >
              {batchConfirm === "purge-all" ? "✅ 确认清空（不可恢复）" : `清空隔离区（${items.length}）`}
            </button>
            {batchConfirm && (
              <button className="ghost" onClick={() => setBatchConfirm(null)}>
                取消
              </button>
            )}
          </>
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
                    {deleteConfirmId === m.id ? (
                      <>
                        <button onClick={() => purgeOne(m.id)}>✅ 确认彻底删除</button>
                        <button className="ghost" onClick={() => setDeleteConfirmId(null)}>
                          取消
                        </button>
                      </>
                    ) : (
                      <>
                        <button className="ghost" onClick={() => restore(m.id)}>
                          恢复
                        </button>
                        <button className="ghost danger" onClick={() => setDeleteConfirmId(m.id)}>
                          删除
                        </button>
                      </>
                    )}
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

  const [rulesPage, setRulesPage] = useState(1);
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
      <p className="hint">每条可清理项都对应这里的一条规则；发现漏了什么目录请提 issue。</p>
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
            共 {filtered.length} 条规则
          </p>
          {filtered.length > rulesPage * 60 && (
            <button className="ghost" onClick={() => setRulesPage(rulesPage + 1)}>
              显示更多（已显示 {Math.min(rulesPage * 60, filtered.length)} / {filtered.length}）
            </button>
          )}
          {filtered.length === 0 && (
            <div className="empty">
              <b>无匹配规则</b>
              试试放宽平台 / 风险筛选，或清空搜索词
            </div>
          )}
          <div className="rules-list">
            {/* 真机验证发现：每次键入全量重渲染 180 张 details 卡片会压垮
                WKWebContent（白屏、无 JS 错误）。截断渲染 + 展开更多。 */}
            {filtered.slice(0, rulesPage * 60).map((r) => (
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
      <h1>Slimit</h1>
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
      {/* 面板常驻挂载、用 display 切换（种子反馈：切 tab 后扫描数据丢失）。
          状态保活 = 不丢扫描任务/手动条目/筛选条件；各面板挂载时自动加载。 */}
      <div style={{ display: tab === "clean" ? "" : "none" }}>
        <CleanPanel />
      </div>
      <div style={{ display: tab === "quarantine" ? "" : "none" }}>
        <QuarantinePanel active={tab === "quarantine"} />
      </div>
      <div style={{ display: tab === "rules" ? "" : "none" }}>
        <RulesPanel />
      </div>
      <div style={{ display: tab === "advanced" ? "" : "none" }}>
        <AdvancedPanel />
        <VolumePanel />
        <SnapshotsPanel />
      </div>
    </main>
  );
}
