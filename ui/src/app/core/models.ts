export type ProjectStatus = 'stale' | 'active' | 'protected' | 'cleaned' | 'unsafe';
export type DeleteMode = 'permanent' | 'trash';

export interface DiskView {
  name: string;
  mount: string;
  freeBytes: number;
  totalBytes: number;
  freeLabel: string;
  totalLabel: string;
  usedRatio: number;
  reclaimableRatio: number;
  freePercent: number;
}

export interface Dashboard {
  reclaimableBytes: number;
  reclaimableLabel: string;
  freed30Bytes: number;
  freed30Label: string;
  freedAllBytes: number;
  freedAllLabel: string;
  projectCount: number;
  staleCount: number;
  nextSweepLabel: string;
  nextSweepSub: string;
  rootsLabel: string;
  lastScanLabel: string;
  disk: DiskView;
  paused: boolean;
  pauseLabel: string;
  onboarded: boolean;
  confirmedRealSweep: boolean;
  scheduleTime: string;
}

export interface Badge { id: string; label: string; tone: string; }

export interface ArtifactView {
  relPath: string;
  pattern: string;
  ruleId: string;
  sizeBytes: number;
  sizeLabel: string;
  kind: string;
  deletable: boolean;
  reason: string | null;
}

export interface ProjectView {
  id: number;
  name: string;
  path: string;
  rootPath: string;
  ecosystem: string;
  badges: Badge[];
  lastTouched: number | null;
  lastTouchedLabel: string;
  artifactSummary: string;
  sizeBytes: number;
  sizeLabel: string;
  status: ProjectStatus;
  protected: boolean;
  gitDirty: boolean;
  lockfile: string | null;
  lockOk: boolean;
  openReason: string | null;
  unsafeReason: string | null;
  action: string;
  artifacts: ArtifactView[];
}

export interface EcosystemSetting { enabled: boolean; optIn: string[]; }

export interface RootConfig {
  path: string;
  enabled: boolean;
  maxDepth: number;
  thresholdDays: number | null;
}

export interface Settings {
  onboarded: boolean;
  confirmedRealSweep: boolean;
  thresholdDays: number;
  dirtyCountsAsActive: boolean;
  scheduleTime: string;
  headsUpMinutes: number;
  onlyWhenIdle: boolean;
  idleMinutes: number;
  onlyOnAc: boolean;
  skipFullscreen: boolean;
  catchUp: boolean;
  lowDiskEnabled: boolean;
  lowDiskPercent: number;
  startAtLogin: boolean;
  deleteMode: DeleteMode;
  skipWithoutLockfile: boolean;
  skipOpenProjects: boolean;
  respectMothballFiles: boolean;
  notifications: boolean;
  roots: RootConfig[];
  ecosystems: Record<string, EcosystemSetting>;
  cacheToggles: Record<string, boolean>;
}

export interface RuleCard {
  id: string;
  name: string;
  safe: string[];
  optIn: string[];
  enabled: boolean;
  enabledOptIn: string[];
}

export interface RootView extends RootConfig {
  projectCount: number;
  thresholdLabel: string;
}

export interface SettingsResponse {
  settings: Settings;
  rules: RuleCard[];
  roots: RootView[];
}

export interface PreviewItem {
  label: string;
  pattern: string;
  relPaths: string[];
  sizeBytes: number;
  sizeLabel: string;
  checked: boolean;
}

export interface PreviewGroup {
  name: string;
  path: string;
  ageLabel: string;
  lockfile: string | null;
  lockOk: boolean;
  items: PreviewItem[];
}

export interface SweepPreview {
  thresholdDays: number;
  deleteMode: DeleteMode;
  groups: PreviewGroup[];
  totalBytes: number;
  selectedBytes: number;
  folderCount: number;
  selectedFolders: number;
}

export interface ExcludeItem { path: string; relPath: string; }

export interface SweepQuery {
  dryRun: boolean;
  trigger: string;
  olderThanDays?: number | null;
  projectPath?: string | null;
  exclude: ExcludeItem[];
  alwaysExclude: boolean;
  deleteMode?: DeleteMode | null;
  includeUnsafe: boolean;
}

export interface SweepProgress {
  project: string;
  path: string;
  relPath: string;
  index: number;
  total: number;
  bytesFreed: number;
  bytesTotal: number;
  filesRemoved: number;
}

export interface SweepOutcome {
  dryRun: boolean;
  bytesFreed: number;
  bytesLabel: string;
  projectsCleaned: number;
  errors: number;
  skipped: number;
  skippedSummary: string;
  message: string;
}

export interface HistoryBar { date: string; label: string; bytes: number; bytesLabel: string; }
export interface HistorySweep {
  id: number;
  whenLabel: string;
  trigger: string;
  projectsLabel: string;
  skippedLabel: string;
  freedLabel: string;
  freedBytes: number;
}
export interface HistoryView {
  rangeDays: number;
  totalBytes: number;
  totalLabel: string;
  sweepCount: number;
  allTimeBytes: number;
  allTimeLabel: string;
  bars: HistoryBar[];
  sweeps: HistorySweep[];
}

export interface CacheView {
  id: string;
  name: string;
  detail: string;
  enabled: boolean;
  available: boolean;
  command: string;
  dangerous: boolean;
}

export interface SuggestedRoot { path: string; reason: string; gitRepos: number; }

export interface RecentItem {
  name: string;
  path: string;
  badge: string;
  tone: string;
  detail: string;
  sizeLabel: string;
}

export interface TrayState {
  watchingLabel: string;
  disk: DiskView;
  reclaimableLabel: string;
  reclaimableBytes: number;
  staleProjects: number;
  nextSweepLabel: string;
  nextSweepPrefix: string;
  paused: boolean;
  pauseLabel: string;
  recent: RecentItem[];
}

export interface ScanProgress {
  phase: string;
  root: string;
  current: string;
  projectsFound: number;
  completed: number;
  total: number;
  rootIndex: number;
  rootCount: number;
  filesSeen: number;
}

export interface ScanSummary {
  projects: number;
  stale: number;
  reclaimableBytes: number;
  reclaimableLabel: string;
}

export function formatBytes(bytes: number): string {
  if (!bytes) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  if (unit === 0) return `${Math.round(value)} B`;
  if (unit >= 3) return `${value.toFixed(1)} ${units[unit]}`;
  return value >= 10 ? `${Math.round(value)} ${units[unit]}` : `${value.toFixed(1)} ${units[unit]}`;
}
