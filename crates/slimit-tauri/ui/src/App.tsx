import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  applyPlan,
  explain,
  listQuarantine,
  listSnapshots,
  restoreItem,
  scanAndPlan,
  volumeSummary,
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

/**
 * 清理面板：扫描 → 规则计划 → 用户勾选 → 隔离执行 → 可恢复。
 * 红线：AI 解释仅为提示层；red/非 executable 项永不进入执行列表
 * （Rust 侧 apply 也有一致校验，双重保险）。
 */
function CleanPanel() {
  const [root, setRoot] = useState("");
  const [busy, setBusy] = useState(false);
  const [applying, setApplying] = useState(false);
  const [err, setErr] = useState("");
  const [progress, setProgress] = useState(0);
  const [data, setData] = useState<ScanPlanResponse | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [reports, setReports] = useState<ApplyReport[] | null>(null);
  const [tips, setTips] = useState<Record<string, Explanation>>({});
  const seqRef = useRef(0);

  useEffect(() => {
    const un = listen<ScanProgress>("scan-progress", (e) => {
      if (e.payload.seq === seqRef.current) setProgress(e.payload.files_done);
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  const scan = async () => {
    if (!root.trim() || busy) return;
    setBusy(true);
    setErr("");
    setReports(null);
    setTips({});
    setProgress(0);
    seqRef.current += 1;
    try {
      const res = await scanAndPlan(root.trim(), 20);
      setData(res);
      // 默认只勾选可执行项（executable=false 的 red/advise 永不入选）。
      setSelected(new Set(res.plan.filter((p) => p.executable).map((p) => p.path)));
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
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

  const runApply = async () => {
    if (!data || applying) return;
    const items = data.plan.filter((p) => p.executable && selected.has(p.path));
    if (items.length === 0) return;
    const ok = window.confirm(
      `将把 ${items.length} 个目录迁入隔离区（可完整恢复）：\n\n` +
        items.map((i) => i.path).join("\n"),
    );
    if (!ok) return;
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

  const planBytes = data
    ? data.plan
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
          placeholder="绝对路径，如 /Users/you/Library"
          disabled={busy || applying}
        />
        <button onClick={scan} disabled={busy || applying || !root.trim()}>
          {busy ? `扫描中… ${progress.toLocaleString()} 个条目` : "扫描并生成计划"}
        </button>
      </div>
      {err && <p className="error">{err}</p>}
      {data && (
        <>
          <p className="totals">
            {data.summary.root} — {data.summary.file_count.toLocaleString()} 个文件 ·
            实际占用 {fmtSize(data.summary.actual)} · 规则命中 {data.plan.length} 项
          </p>
          {data.plan.length > 0 && (
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
                  {data.plan.map((p) => (
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
                <button onClick={runApply} disabled={applying || planBytes === 0}>
                  {applying
                    ? "执行中…"
                    : `执行（隔离 ${planBytes ? fmtSize(planBytes) : "0 MiB"}）`}
                </button>
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
            {tip ? "已解释" : "解释"}
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
            {tip.producer}（置信度 {(tip.confidence * 100).toFixed(0)}%）
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

function QuarantinePanel() {
  const [items, setItems] = useState<Manifest[] | null>(null);
  const [err, setErr] = useState("");
  const [restored, setRestored] = useState<string | null>(null);

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

  return (
    <section>
      <h2>隔离区</h2>
      <div className="row">
        <button onClick={run}>列出隔离条目</button>
      </div>
      {err && <p className="error">{err}</p>}
      {restored && <p className="ok">已恢复到：{restored}</p>}
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
    </main>
  );
}
