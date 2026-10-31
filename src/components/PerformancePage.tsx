import { useEffect, useRef, useState } from "react";
import {
  Activity,
  Cpu,
  Database,
  Download,
  HardDrive,
  KeyRound,
  Loader2,
  Network,
  RefreshCw,
  Server,
  Square,
  Upload,
  X,
} from "lucide-react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { api, errorMessage } from "../lib/api";
import type {
  BigKeyInfo,
  PerformanceReport,
  PerformanceSummary,
  ServerInfo,
} from "../lib/types";
import { useAppStore } from "../stores/appStore";
import { useSettingsStore } from "../stores/settingsStore";
import { Portal } from "./Portal";

const typeClass: Record<string, string> = {
  string: "type-string",
  hash: "type-hash",
  list: "type-list",
  set: "type-set",
  zset: "type-zset",
  stream: "type-stream",
};

function number(value: number) {
  return Math.round(value).toLocaleString("zh-CN");
}

function modeLabel(mode: string) {
  if (mode === "standalone") return "单机";
  if (mode === "sentinel") return "Sentinel";
  if (mode === "cluster") return "Cluster";
  return mode || "单机";
}

function uptime(seconds: number) {
  if (!seconds) return "-";
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (days > 0) return `${days} 天 ${hours} 小时`;
  if (hours > 0) return `${hours} 小时 ${minutes} 分`;
  return `${minutes} 分`;
}

function hitRate(server: ServerInfo | undefined) {
  if (!server) return 0;
  const total = server.keyspaceHits + server.keyspaceMisses;
  return total > 0 ? (server.keyspaceHits / total) * 100 : 0;
}

function formatThreshold(bytes: number) {
  const gb = 1024 * 1024 * 1024;
  const mb = 1024 * 1024;
  if (bytes >= gb && bytes % gb === 0) return `${bytes / gb} GB`;
  if (bytes >= mb && bytes % mb === 0) return `${bytes / mb} MB`;
  if (bytes >= 1024 && bytes % 1024 === 0) return `${bytes / 1024} KB`;
  return `${bytes} B`;
}

function humanBytes(bytes: number) {
  const gb = 1024 * 1024 * 1024;
  const mb = 1024 * 1024;
  if (bytes >= gb) return `${(bytes / gb).toFixed(2)} GB`;
  if (bytes >= mb) return `${(bytes / mb).toFixed(2)} MB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${Math.round(bytes)} B`;
}

function AnimatedNumber({
  value,
  format,
  duration = 620,
  className = "",
}: {
  value: number;
  format: (value: number) => string;
  duration?: number;
  className?: string;
}) {
  const [display, setDisplay] = useState(value);
  const currentRef = useRef(value);

  useEffect(() => {
    const from = currentRef.current;
    const to = value;
    if (!Number.isFinite(to)) return;
    if (from === to) return;
    let frame = 0;
    const started = performance.now();
    const tick = (now: number) => {
      const progress = Math.min(1, (now - started) / duration);
      const eased = 1 - Math.pow(1 - progress, 3);
      const next = from + (to - from) * eased;
      currentRef.current = next;
      setDisplay(next);
      if (progress < 1) {
        frame = requestAnimationFrame(tick);
      } else {
        currentRef.current = to;
        setDisplay(to);
      }
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [value, duration]);

  return (
    <span className={`perf-animated-number ${className}`}>
      {format(display)}
    </span>
  );
}

function BigKeyRow({ item, index }: { item: BigKeyInfo; index: number }) {
  return (
    <div className="perf-big-row">
      <span className="perf-rank">{index + 1}</span>
      <span className="perf-db">db{item.db}</span>
      <span className="mono perf-key" title={item.key}>
        {item.key}
      </span>
      <span className={`type-badge ${typeClass[item.keyType] ?? ""}`}>
        {item.keyType}
      </span>
      <span className="perf-memory">
        <AnimatedNumber value={item.memory} format={humanBytes} />
      </span>
    </div>
  );
}

function StatCard({
  icon,
  label,
  value,
  meta,
}: {
  icon: React.ReactNode;
  label: string;
  value: React.ReactNode;
  meta?: React.ReactNode;
}) {
  return (
    <div className="perf-stat-card">
      <span className="perf-stat-icon">{icon}</span>
      <div>
        <span className="perf-stat-label">{label}</span>
        <strong>{value}</strong>
        {meta && <small>{meta}</small>}
      </div>
    </div>
  );
}

export function PerformancePage() {
  const activeProfileId = useAppStore((state) => state.activeProfileId);
  const connection = useAppStore((state) => state.connection);
  const readOnly = useAppStore((state) => state.readOnly);
  const refreshKeys = useAppStore((state) => state.refreshKeys);
  const refreshDatabases = useAppStore((state) => state.refreshDatabases);
  const showNotice = useAppStore((state) => state.showNotice);
  const bigKeyThresholdBytes = useSettingsStore(
    (state) => state.bigKeyThresholdBytes,
  );

  const [report, setReport] = useState<PerformanceReport | null>(null);
  const [summary, setSummary] = useState<PerformanceSummary | null>(null);
  const [scanning, setScanning] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [lastScanAt, setLastScanAt] = useState<Date | null>(null);
  const [busyAction, setBusyAction] = useState<"export" | "import" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [importPath, setImportPath] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);
  const scanningRef = useRef(false);

  // 轻量轮询：只读 INFO + PING，代价与键数量无关。
  const refreshSummary = async () => {
    if (!activeProfileId) return;
    try {
      const next = await api.getPerformanceSummary(activeProfileId);
      setSummary(next);
      setError(null);
    } catch (err) {
      setError(errorMessage(err));
    }
  };

  // 深度扫描：逐键统计内存和大 Key，只在进入页面、手动点击时执行。
  const runDeepScan = async () => {
    if (!activeProfileId || scanningRef.current) return;
    scanningRef.current = true;
    setScanning(true);
    setError(null);
    try {
      const result = await api.analyzePerformance(activeProfileId);
      setReport(result);
      setLastScanAt(new Date());
    } catch (err) {
      const message = errorMessage(err);
      if (message.includes("扫描已取消")) {
        showNotice("info", "已停止深度扫描");
      } else {
        setError(message);
      }
    } finally {
      scanningRef.current = false;
      setScanning(false);
      setCancelling(false);
    }
  };

  const cancelScan = async () => {
    if (!activeProfileId || !scanning) return;
    setCancelling(true);
    try {
      await api.cancelPerformanceScan(activeProfileId);
    } catch (err) {
      setError(errorMessage(err));
    }
  };

  useEffect(() => {
    void refreshSummary();
    void runDeepScan();
    const timer = window.setInterval(() => {
      void refreshSummary();
    }, 3000);
    return () => {
      window.clearInterval(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeProfileId]);

  const rate = hitRate(summary?.server ?? report?.server);
  const server = summary?.server ?? report?.server;
  const latencyMs = summary?.latencyMs ?? report?.latencyMs ?? 0;
  const totalKeys = summary?.totalKeys ?? report?.totalKeys ?? 0;
  const liveKeys = new Map<number, number>();
  for (const info of server?.keyspace ?? []) liveKeys.set(info.db, info.keys);

  const thresholdBytes = bigKeyThresholdBytes;
  const bigKeys = (report?.bigKeys ?? []).filter(
    (item) => item.memory >= thresholdBytes,
  );

  const exportAll = async () => {
    if (!activeProfileId) return;
    try {
      const path = await save({
        defaultPath: "leonredis-export.json",
        filters: [{ name: "LeonRedis 导出文件", extensions: ["json"] }],
      });
      if (!path) return;
      setBusyAction("export");
      setError(null);
      const result = await api.exportData(activeProfileId, path);
      showNotice(
        "success",
        `已导出 ${result.keys} 个键（${result.databases} 个数据库）`,
      );
    } catch (err) {
      const message = errorMessage(err);
      setError(message);
      showNotice("error", message);
    } finally {
      setBusyAction(null);
    }
  };

  const chooseImport = async () => {
    if (!activeProfileId || readOnly) return;
    try {
      const path = await open({
        multiple: false,
        filters: [{ name: "LeonRedis 导出文件", extensions: ["json"] }],
      });
      if (typeof path === "string" && path) setImportPath(path);
    } catch (err) {
      setError(errorMessage(err));
    }
  };

  const confirmImport = async () => {
    if (!activeProfileId || !importPath) return;
    setImporting(true);
    setError(null);
    try {
      const result = await api.importData(activeProfileId, importPath);
      showNotice(
        "success",
        `已导入 ${result.keys} 个键（${result.databases} 个数据库）`,
      );
      setImportPath(null);
      await Promise.all([refreshKeys(), refreshDatabases()]);
      await refreshSummary();
      await runDeepScan();
    } catch (err) {
      const message = errorMessage(err);
      setError(message);
      showNotice("error", message);
    } finally {
      setImporting(false);
    }
  };

  const hasData = Boolean(report || summary);

  return (
    <section className="performance-page">
      <div className="performance-head">
        <div>
          <h2>性能分析</h2>
          <p>
            指标每 3 秒自动刷新；内存明细与大 Key 需要深度扫描
            {lastScanAt && ` · 上次深度扫描 ${lastScanAt.toLocaleTimeString("zh-CN")}`}
          </p>
        </div>
        <div className="performance-actions">
          {scanning ? (
            <button
              className="btn ghost"
              onClick={() => void cancelScan()}
              disabled={cancelling}
            >
              {cancelling ? (
                <Loader2 size={16} className="spin" />
              ) : (
                <Square size={15} />
              )}
              停止扫描
            </button>
          ) : (
            <button
              className="btn ghost"
              onClick={() => void runDeepScan()}
              disabled={!activeProfileId}
            >
              <RefreshCw size={16} />
              {report ? "重新深度扫描" : "深度扫描"}
            </button>
          )}
          <button
            className="btn ghost"
            onClick={() => void exportAll()}
            disabled={busyAction !== null || !report}
          >
            {busyAction === "export" ? (
              <Loader2 size={16} className="spin" />
            ) : (
              <Download size={16} />
            )}
            导出全部
          </button>
          <button
            className="btn ghost"
            onClick={() => void chooseImport()}
            disabled={busyAction !== null || readOnly}
            title={readOnly ? "只读模式已开启" : "导入全部数据"}
          >
            {busyAction === "import" ? (
              <Loader2 size={16} className="spin" />
            ) : (
              <Upload size={16} />
            )}
            导入数据
          </button>
        </div>
      </div>

      <div className="performance-scroll">
        {error && <div className="perf-error">{error}</div>}

        {!hasData ? (
          <div className="perf-loading">
            <Loader2 size={22} className="spin" />
            <span>正在读取服务器信息…</span>
          </div>
        ) : (
          <>
            <div className="perf-quad-grid">
              <section className="perf-quad perf-quad-deploy">
                <div className="perf-quad-head">
                  <h3>部署服务器</h3>
                  <span>
                    {connection?.name} · {connection?.host}:{connection?.port}
                  </span>
                </div>
                <div className="perf-deploy-grid">
                  <StatCard
                    icon={<Server size={19} />}
                    label="系统"
                    value={server?.os || "未知"}
                    meta={
                      server?.archBits
                        ? `${server.archBits} 位 · 运行 ${uptime(server.uptimeInSeconds)}`
                        : "运行时间未知"
                    }
                  />
                  <StatCard
                    icon={<Database size={19} />}
                    label="Redis 版本"
                    value={`v${server?.version ?? connection?.version ?? "-"}`}
                    meta={`${modeLabel(server?.mode ?? connection?.mode ?? "")} · ${
                      server?.role ?? "master"
                    }`}
                  />
                  <StatCard
                    icon={<HardDrive size={19} />}
                    label="Redis 内存用量"
                    value={
                      <AnimatedNumber
                        value={server?.usedMemory ?? report?.usedMemory ?? 0}
                        format={humanBytes}
                      />
                    }
                    meta={
                      server && server.maxmemory > 0 ? (
                        <>上限 {server.maxmemoryHuman}</>
                      ) : (
                        <>
                          键内存{" "}
                          <AnimatedNumber
                            value={report?.databasesMemory ?? 0}
                            format={humanBytes}
                          />
                        </>
                      )
                    }
                  />
                  <StatCard
                    icon={<Network size={19} />}
                    label="网络延迟"
                    value={
                      <AnimatedNumber
                        value={latencyMs}
                        format={(value) => `${Math.round(value)} ms`}
                      />
                    }
                    meta={
                      <>
                        <AnimatedNumber value={totalKeys} format={number} /> 键 ·{" "}
                        {report?.databases.length ?? server?.keyspace.length ?? 0}{" "}
                        个数据库
                      </>
                    }
                  />
                </div>
              </section>

              <section className="perf-quad perf-quad-service">
                <div className="perf-quad-head">
                  <h3>Redis 服务性能</h3>
                  <span>运行 {uptime(server?.uptimeInSeconds ?? 0)}</span>
                </div>
                <div className="perf-service-grid">
                  <div className="perf-service-item">
                    <Activity size={18} />
                    <span>Ops / 秒</span>
                    <strong>
                      <AnimatedNumber
                        value={server?.instantaneousOpsPerSec ?? 0}
                        format={number}
                      />
                    </strong>
                  </div>
                  <div className="perf-service-item">
                    <KeyRound size={18} />
                    <span>缓存命中率</span>
                    <strong>
                      <AnimatedNumber
                        value={rate}
                        format={(value) => `${value.toFixed(1)}%`}
                      />
                    </strong>
                  </div>
                  <div className="perf-service-item">
                    <Server size={18} />
                    <span>客户端</span>
                    <strong>
                      <AnimatedNumber
                        value={server?.connectedClients ?? 0}
                        format={number}
                      />
                    </strong>
                    <small>
                      阻塞{" "}
                      <AnimatedNumber
                        value={server?.blockedClients ?? 0}
                        format={number}
                      />
                    </small>
                  </div>
                  <div className="perf-service-item">
                    <Cpu size={18} />
                    <span>累计命令</span>
                    <strong>
                      <AnimatedNumber
                        value={server?.totalCommandsProcessed ?? 0}
                        format={number}
                      />
                    </strong>
                    <small>
                      CPU{" "}
                      <AnimatedNumber
                        value={
                          (server?.usedCpuSys ?? 0) +
                          (server?.usedCpuUser ?? 0)
                        }
                        format={(value) => `${value.toFixed(1)}s`}
                      />
                    </small>
                  </div>
                </div>
              </section>

              <section className="perf-quad perf-quad-db">
                <div className="perf-quad-head">
                  <h3>各数据库内存用量</h3>
                  <span>
                    <AnimatedNumber
                      value={report?.databasesMemory ?? 0}
                      format={humanBytes}
                    />
                  </span>
                </div>
                {!report ? (
                  <div className="perf-empty">
                    {scanning ? "正在深度扫描内存明细…" : "尚未执行深度扫描"}
                  </div>
                ) : report.databases.length === 0 ? (
                  <div className="perf-empty">当前连接没有数据</div>
                ) : (
                  <div className="perf-quad-table perf-db-table">
                    <div className="perf-db-head">
                      <span>数据库</span>
                      <span>键数</span>
                      <span>内存</span>
                      <span>占比</span>
                    </div>
                    {report.databases.map((db) => (
                      <div className="perf-db-row" key={db.db}>
                        <span className="perf-db">db{db.db}</span>
                        <span>
                          <AnimatedNumber
                            value={liveKeys.get(db.db) ?? db.keys}
                            format={number}
                          />
                        </span>
                        <span className="perf-memory">
                          <AnimatedNumber
                            value={db.memory}
                            format={humanBytes}
                          />
                        </span>
                        <span className="perf-bar-wrap">
                          <span className="perf-bar-track">
                            <span
                              className="perf-bar"
                              style={{ width: `${Math.max(db.percent, 0)}%` }}
                            />
                          </span>
                          <AnimatedNumber
                            value={db.percent}
                            format={(value) => `${value.toFixed(1)}%`}
                          />
                        </span>
                      </div>
                    ))}
                  </div>
                )}
              </section>

              <section className="perf-quad perf-quad-big">
                <div className="perf-quad-head">
                  <h3>大 Key 扫描</h3>
                  <span>
                    阈值 ≥ {formatThreshold(bigKeyThresholdBytes)} · 最多 20 个
                  </span>
                </div>
                {!report ? (
                  <div className="perf-empty">
                    {scanning ? "正在深度扫描大 Key…" : "尚未执行深度扫描"}
                  </div>
                ) : bigKeys.length === 0 ? (
                  <div className="perf-empty">没有达到阈值的键</div>
                ) : (
                  <div className="perf-quad-table perf-big-table">
                    <div className="perf-big-head">
                      <span>#</span>
                      <span>数据库</span>
                      <span>键名</span>
                      <span>类型</span>
                      <span>内存</span>
                    </div>
                    {bigKeys.map((item, index) => (
                      <BigKeyRow
                        key={`${item.db}:${item.key}`}
                        item={item}
                        index={index}
                      />
                    ))}
                  </div>
                )}
              </section>
            </div>

            <p className="perf-footnote">
              内存来自 Redis MEMORY USAGE 的逐键统计，仅在深度扫描时更新，可能与
              INFO 总内存存在开销差异。
            </p>
          </>
        )}
      </div>

      {importPath && (
        <Portal>
          <div
            className="modal-backdrop"
            onMouseDown={() => !importing && setImportPath(null)}
          >
            <div
              className="modal modal-small"
              onMouseDown={(event) => event.stopPropagation()}
            >
              <div className="modal-head">
                <h3>导入全部数据</h3>
                <button
                  className="icon-btn"
                  onClick={() => setImportPath(null)}
                  disabled={importing}
                >
                  <X size={16} />
                </button>
              </div>
              <div className="modal-body">
                <div className="danger-box">
                  <span>
                    将从文件导入所有数据库的数据。相同数据库、相同键名的键会被覆盖，
                    其他键会保留。
                  </span>
                </div>
                <p className="perf-import-path" title={importPath}>
                  {importPath}
                </p>
              </div>
              <div className="modal-footer">
                <button
                  className="btn ghost"
                  onClick={() => setImportPath(null)}
                  disabled={importing}
                >
                  取消
                </button>
                <button
                  className="btn danger"
                  onClick={() => void confirmImport()}
                  disabled={importing}
                >
                  {importing ? (
                    <Loader2 size={14} className="spin" />
                  ) : (
                    <Upload size={14} />
                  )}
                  确认导入
                </button>
              </div>
            </div>
          </div>
        </Portal>
      )}
    </section>
  );
}
