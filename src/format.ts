import type { Project } from "./types";

/** Human-readable byte sizes. */
export function formatBytes(bytes: number): string {
  if (!bytes || bytes < 1) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(
    units.length - 1,
    Math.floor(Math.log(bytes) / Math.log(1024))
  );
  const value = bytes / Math.pow(1024, i);
  return `${value.toFixed(value >= 100 || i === 0 ? 0 : 1)} ${units[i]}`;
}

/** Relative "last activity" label from a unix timestamp (seconds). */
export function formatActivity(unixSecs: number): string {
  if (!unixSecs) return "unknown";
  const days = Math.floor((Date.now() / 1000 - unixSecs) / 86400);
  if (days <= 0) return "today";
  if (days === 1) return "1 day ago";
  if (days < 30) return `${days} days ago`;
  if (days < 60) return "1 month ago";
  if (days < 365) return `${Math.floor(days / 30)} months ago`;
  const years = Math.floor(days / 365);
  return years === 1 ? "1 year ago" : `${years} years ago`;
}

/** Health score → colour band. */
export function healthColor(score: number): string {
  if (score >= 80) return "#16a34a";
  if (score >= 50) return "#d97706";
  return "#dc2626";
}

export type ActivityBucket = "active" | "dormant" | "archived";

/** Bucket a project by recency of activity (matches the Rust thresholds). */
export function activityBucket(unixSecs: number): ActivityBucket {
  if (!unixSecs) return "archived";
  const days = (Date.now() / 1000 - unixSecs) / 86400;
  if (days <= 30) return "active";
  if (days <= 180) return "dormant";
  return "archived";
}

export interface HealthFactor {
  label: string;
  delta: number; // signed points; 0 = informational/positive note
  good: boolean;
}

/**
 * Explain a project's health score. This mirrors `src-tauri/src/health.rs`
 * exactly — keep the two in sync if the formula changes.
 */
export function healthFactors(p: Project): HealthFactor[] {
  const factors: HealthFactor[] = [];

  // Git
  if (p.git_present) {
    factors.push({ label: "Git repository", delta: 0, good: true });
  } else {
    factors.push({ label: "No git repository", delta: -20, good: false });
  }

  // README
  if (p.has_readme) {
    factors.push({ label: "Has README", delta: 0, good: true });
  } else {
    factors.push({ label: "No README", delta: -10, good: false });
  }

  // Junk ratio (up to -30)
  if (p.size_bytes > 0 && p.junk_bytes > 0) {
    const ratio = p.junk_bytes / p.size_bytes;
    const penalty = Math.round(Math.min(ratio * 30, 30));
    if (penalty > 0) {
      factors.push({
        label: `${formatBytes(p.junk_bytes)} junk (${Math.round(ratio * 100)}% of size)`,
        delta: -penalty,
        good: false,
      });
    }
  } else if (p.junk_bytes === 0) {
    factors.push({ label: "No junk", delta: 0, good: true });
  }

  // Activity
  if (p.last_activity > 0) {
    const days = Math.floor((Date.now() / 1000 - p.last_activity) / 86400);
    if (days > 180) {
      factors.push({ label: "Inactive over 180 days", delta: -25, good: false });
    } else if (days > 90) {
      factors.push({ label: "Inactive 90–180 days", delta: -15, good: false });
    } else if (days > 30) {
      factors.push({ label: "Inactive 30–90 days", delta: -5, good: false });
    } else {
      factors.push({ label: "Active in last 30 days", delta: 5, good: true });
    }
  } else {
    factors.push({ label: "Activity unknown", delta: -15, good: false });
  }

  return factors;
}

/** Short date label from a unix timestamp (seconds), e.g. "14 Jun". */
export function formatDate(unixSecs: number): string {
  if (!unixSecs) return "-";
  return new Date(unixSecs * 1000).toLocaleDateString(undefined, {
    day: "numeric",
    month: "short",
  });
}

/** Date + time label, e.g. "14 Jun, 18:32". */
export function formatDateTime(unixSecs: number): string {
  if (!unixSecs) return "-";
  return new Date(unixSecs * 1000).toLocaleString(undefined, {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** Confidence score → colour band. */
export function confidenceColor(c: number): string {
  if (c >= 70) return "#16a34a";
  if (c >= 40) return "#d97706";
  return "#dc2626";
}

export type Risk = "low" | "medium" | "high";

export interface CleanupInfo {
  risk: Risk;
  confidence: number; // 0-100, how safe to remove
  regenerable: boolean;
}

const CLEANUP: Record<string, CleanupInfo> = {
  node_modules: { risk: "low", confidence: 99, regenerable: true },
  ".next": { risk: "low", confidence: 99, regenerable: true },
  coverage: { risk: "low", confidence: 99, regenerable: true },
  __pycache__: { risk: "low", confidence: 99, regenerable: true },
  dist: { risk: "low", confidence: 95, regenerable: true },
  build: { risk: "low", confidence: 95, regenerable: true },
  target: { risk: "low", confidence: 95, regenerable: true },
  obj: { risk: "low", confidence: 95, regenerable: true },
  bin: { risk: "low", confidence: 90, regenerable: true },
  ".venv": { risk: "low", confidence: 90, regenerable: true },
  logs: { risk: "medium", confidence: 60, regenerable: false },
  archives: { risk: "medium", confidence: 25, regenerable: false },
};

/** Cleanup classification for a junk category. Unknown = treat as high risk. */
export function cleanupInfo(name: string): CleanupInfo {
  return CLEANUP[name] ?? { risk: "high", confidence: 0, regenerable: false };
}

export function riskColor(risk: Risk): string {
  if (risk === "low") return "#16a34a";
  if (risk === "medium") return "#d97706";
  return "#dc2626";
}

export function riskLabel(risk: Risk): string {
  if (risk === "low") return "Safe";
  if (risk === "medium") return "Review";
  return "Caution";
}
