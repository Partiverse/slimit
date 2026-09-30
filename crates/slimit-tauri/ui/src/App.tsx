import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  applyPlan,
  explain,
  getSettings,
  listQuarantine,
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
  const seqRef = useRef(0);

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
          new Set(result.plan.filter((p) => p.executable).map((p) => p.path)),
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
      });
      setTips((t) => ({ ...t, [p.path]: e }));
    } catch (err) {
      setErr(String(err));
    }
  };

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
          placeholder="绝对路径，可先后提交多个扫描任务并发执行"
          disabled={anyRunning && false}
        />
        <button onClick={startScan} disabled={!root.trim()}>
          开始扫描
        </button>
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
          {selectedTask.result.plan.length > 0 && (
            <>
              <table>
                <thead>
                  <tr>
                    <th></th>
                    <th>risk</th>
                    <th>可回收</th>
                    <th>rule</th>
                    <th>path</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  {selectedTask.result.plan.map((p) => (
                    <PlanRow
                      key={p.rule_id + p.path}
                      item={p}
                      checked={selected.has(p.path)}
                      tip={tips[p.path]}
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
  onToggle: () => void;
  onExplain: () => void;
}) {
  const { item: p, checked, tip, onToggle, onExplain } = props;
  return (
    <>
      <tr>
        <td>
          <input
            type="checkbox"
            checked={checked}
            disabled={!p.executable}
            onChange={onToggle}
          />
        </td>
        <td>{RISK_LABEL[p.risk]}</td>
        <td>{fmtSize(p.estimated_bytes)}</td>
        <td className="path">{p.rule_id}</td>
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
        <button onClick={run}>列出隔离条目</button>
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
      {items && (
        <>
          <p>{items.length} 个条目</p>
          <table>
            <thead>
              <tr>
                <th>原路径</th>
                <th>大小</th>
                <th>rule</th>
                <th>时间</th>
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

export default function App() {
  return (
    <main>
      <h1>SlimIt</h1>
      <p className="hint">
        扫描 → 规则计划 → 隔离执行 → 可恢复；AI 解释仅作提示，永不影响执行
      </p>
      <CleanPanel />
      <QuarantinePanel />
      <VolumePanel />
      <SnapshotsPanel />
      <AiSettingsPanel />
    </main>
  );
}
