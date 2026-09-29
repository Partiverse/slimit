import { useState } from "react";
import {
  listSnapshots,
  scanDir,
  volumeSummary,
  type ScanSummary,
  type SnapshotInfo,
  type VolumeSummary,
} from "./bridge";

const GIB = 1024 ** 3;
const fmtGiB = (b: number | null | undefined) =>
  b == null ? "—" : (b / GIB).toFixed(2) + " GiB";
const fmtMiB = (b: number) => (b / (1024 * 1024)).toFixed(2) + " MiB";

function ScanPanel() {
  const [root, setRoot] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");
  const [summary, setSummary] = useState<ScanSummary | null>(null);

  const run = async () => {
    if (!root.trim() || busy) return;
    setBusy(true);
    setErr("");
    try {
      setSummary(await scanDir(root.trim(), 20));
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section>
      <h2>扫描</h2>
      <div className="row">
        <input
          value={root}
          onChange={(e) => setRoot(e.target.value)}
          placeholder="绝对路径，如 /Users/you/Downloads"
          disabled={busy}
        />
        <button onClick={run} disabled={busy || !root.trim()}>
          {busy ? "扫描中…" : "扫描"}
        </button>
      </div>
      {err && <p className="error">{err}</p>}
      {summary && (
        <>
          <p className="totals">
            {summary.root} — {summary.file_count} 个文件 · 实际占用{" "}
            {fmtMiB(summary.actual)} · 表观 {fmtMiB(summary.apparent)}
          </p>
          <table>
            <thead>
              <tr>
                <th>actual</th>
                <th>apparent</th>
                <th>files</th>
                <th>path</th>
              </tr>
            </thead>
            <tbody>
              {summary.top_dirs.map((d) => (
                <tr key={d.path}>
                  <td>{fmtMiB(d.actual)}</td>
                  <td>{fmtMiB(d.apparent)}</td>
                  <td>{d.file_count}</td>
                  <td className="path">{d.path}</td>
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
      <p className="hint">W5 骨架：扫描 / 卷容量 / APFS 快照三面板</p>
      <ScanPanel />
      <VolumePanel />
      <SnapshotsPanel />
    </main>
  );
}
