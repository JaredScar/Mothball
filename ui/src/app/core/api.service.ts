import { Injectable, NgZone, inject } from '@angular/core';
import {
  CacheView, Dashboard, ExcludeItem, HistoryView, ProjectView, ScanProgress, ScanSummary, Settings, SettingsResponse,
  SuggestedRoot, SweepOutcome, SweepPreview, SweepProgress, SweepQuery, TrayState, formatBytes,
} from './models';

interface TauriWindow {
  __TAURI_INTERNALS__?: unknown;
  __MOTHBALL_WINDOW__?: string;
}

@Injectable({ providedIn: 'root' })
export class ApiService {
  private readonly zone = inject(NgZone);
  readonly inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
  readonly windowKind: 'main' | 'tray' | 'toast';

  private demo = createDemo();

  constructor() {
    const win = window as unknown as TauriWindow;
    const query = new URLSearchParams(location.search).get('window');
    const kind = win.__MOTHBALL_WINDOW__ || query || 'main';
    this.windowKind = kind === 'tray' || kind === 'toast' ? kind : 'main';
  }

  dashboard(): Promise<Dashboard> {
    return this.call('get_dashboard', undefined, () => this.demo.dashboard());
  }

  tray(): Promise<TrayState> {
    return this.call('get_tray', undefined, () => this.demo.tray());
  }

  projects(): Promise<ProjectView[]> {
    return this.call('list_projects', undefined, () => this.demo.projects.map((project) => structuredClone(project)));
  }

  async scan(onProgress?: (progress: ScanProgress) => void): Promise<ScanSummary> {
    if (!this.inTauri) {
      if (onProgress) await demoScan(onProgress);
      this.demo.lastScanLabel = 'just now';
      const stale = this.demo.projects.filter((project) => project.status === 'stale').length;
      return { projects: this.demo.projects.length, stale, reclaimableBytes: 0, reclaimableLabel: '0 B' };
    }
    const unlisten = onProgress ? await this.on<ScanProgress>('scan-progress', onProgress) : () => undefined;
    try {
      return await this.invoke<ScanSummary>('scan_roots');
    } finally {
      unlisten();
    }
  }

  preview(query: Partial<SweepQuery> = {}): Promise<SweepPreview> {
    return this.call('preview_sweep', { query: fullQuery(query) }, () => this.demo.preview(query));
  }

  async sweep(query: Partial<SweepQuery>, onProgress?: (progress: SweepProgress) => void): Promise<SweepOutcome> {
    if (!this.inTauri) {
      if (onProgress) await demoClean(onProgress);
      return this.demo.sweep(query);
    }
    const unlisten = onProgress ? await this.on<SweepProgress>('sweep-progress', onProgress) : () => undefined;
    try {
      return await this.invoke<SweepOutcome>('run_sweep', { query: fullQuery(query) });
    } finally {
      unlisten();
    }
  }

  cancelSweep(): Promise<void> {
    return this.call('cancel_sweep', undefined, async () => undefined);
  }

  rehydrate(path: string, onLine: (line: string) => void): Promise<void> {
    if (!this.inTauri) {
      onLine(`$ npm ci`);
      onLine(`added 412 packages in 8s`);
      onLine('Restore finished.');
      const project = this.demo.projects.find((item) => item.path === path);
      if (project && project.status === 'cleaned') {
        project.status = 'active';
        project.action = 'protect';
        project.artifactSummary = 'node_modules';
        project.sizeBytes = 180 * 1024 * 1024;
        project.sizeLabel = formatBytes(project.sizeBytes);
        project.lastTouchedLabel = 'Just now';
      }
      return Promise.resolve();
    }
    return this.listenCommand('rehydrate', { path }, 'rehydrate-line', onLine);
  }

  setProtected(path: string, protectedFlag: boolean): Promise<void> {
    return this.call('set_protected', { path, protected: protectedFlag }, async () => {
      const project = this.demo.projects.find((item) => item.path === path);
      if (!project) return;
      project.protected = protectedFlag;
      project.status = protectedFlag ? 'protected' : 'stale';
      project.action = protectedFlag ? 'unprotect' : 'clean';
    });
  }

  setAllowUnsafe(path: string, allow: boolean): Promise<void> {
    return this.call('set_allow_unsafe', { path, allow }, async () => {
      const project = this.demo.projects.find((item) => item.path === path);
      if (project && allow) {
        project.status = 'stale';
        project.action = 'clean';
        project.unsafeReason = null;
      }
    });
  }

  history(days: number): Promise<HistoryView> {
    return this.call('get_history', { days }, () => this.demo.history(days));
  }

  async exportHistory(): Promise<void> {
    if (this.inTauri) {
      await this.invoke('export_history');
      return;
    }
    const csv = 'when,trigger,project,path,folder,size_bytes,result,error\n';
    const blob = new Blob([csv], { type: 'text/csv' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = 'mothball-history.csv';
    link.click();
    URL.revokeObjectURL(url);
  }

  settings(): Promise<SettingsResponse> {
    return this.call('get_settings', undefined, () => this.demo.settingsView());
  }

  saveSettings(settings: Settings): Promise<void> {
    return this.call('save_settings', { settings }, async () => {
      const saved = structuredClone(settings) as unknown as Record<string, unknown>;
      const current = this.demo.settings as unknown as Record<string, unknown>;
      for (const key of Object.keys(current)) delete current[key];
      Object.assign(current, saved);
    });
  }

  addRoot(path: string): Promise<void> {
    return this.call('add_root', { path }, async () => {
      if (!this.demo.settings.roots.some((root) => root.path === path)) {
        this.demo.settings.roots.push({ path, enabled: true, maxDepth: 6, thresholdDays: null });
      }
    });
  }

  removeRoot(path: string): Promise<void> {
    return this.call('remove_root', { path }, async () => {
      this.demo.settings.roots = this.demo.settings.roots.filter((root) => root.path !== path);
    });
  }

  pickFolder(): Promise<string | null> {
    return this.call('pick_folder', undefined, async () => null);
  }

  suggestRoots(): Promise<SuggestedRoot[]> {
    return this.call('suggest_roots', undefined, async () => [
      { path: 'C:\\dev', reason: '12 git repos', gitRepos: 12 },
      { path: 'C:\\work', reason: 'Common location', gitRepos: 4 },
      { path: 'C:\\fivem\\resources', reason: '7 git repos', gitRepos: 7 },
    ]);
  }

  caches(): Promise<CacheView[]> {
    return this.call('get_caches', undefined, () => this.demo.caches());
  }

  pruneCache(id: string, confirm: boolean): Promise<string> {
    return this.call('prune_cache', { id, confirm }, async () => `${id} finished.`);
  }

  toast(): Promise<{ kind: string; title: string; body: string }> {
    const params = new URLSearchParams(location.search);
    return this.call('get_toast', undefined, async () => ({
      kind: params.get('toast') || 'heads-up',
      title: params.get('title') || 'Sweep starts at 11:00 PM',
      body: params.get('body') || 'Stale projects will be cleaned. Only regenerable folders are deleted.',
    }));
  }

  setPause(until: string): Promise<void> {
    return this.call('set_pause', { until }, async () => {
      this.demo.paused = until !== 'resume';
      this.demo.pauseLabel = until === 'forever' ? 'Until you resume' : until === 'resume' ? '' : 'Paused';
    });
  }

  async on<T>(event: string, handler: (payload: T) => void): Promise<() => void> {
    if (!this.inTauri) return () => undefined;
    const mod = await import('@tauri-apps/api/event');
    const unlisten = await mod.listen<T>(event, (message) => this.zone.run(() => handler(message.payload)));
    return unlisten;
  }

  private async call<T>(command: string, args: Record<string, unknown> | undefined, fallback: () => Promise<T> | T): Promise<T> {
    if (!this.inTauri) return await fallback();
    return this.invoke<T>(command, args);
  }

  private async invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    const mod = await import('@tauri-apps/api/core');
    return mod.invoke<T>(command, args);
  }

  private async listenCommand(command: string, args: Record<string, unknown>, event: string, onLine: (line: string) => void): Promise<void> {
    const unlisten = await this.on<{ line: string }>(event, (payload) => onLine(payload.line));
    try {
      await this.invoke(command, args);
    } finally {
      unlisten();
    }
  }
}

async function demoClean(onProgress: (progress: SweepProgress) => void) {
  const total = 4;
  for (let step = 1; step <= total; step++) {
    onProgress({
      project: 'hivemind-games',
      path: 'C:\\dev\\hivemind\\node_modules',
      relPath: 'node_modules',
      index: step,
      total,
      bytesFreed: step === 1 ? 0 : (step - 1) * 2_000_000_000,
      bytesTotal: 8_000_000_000,
      filesRemoved: step * 14000,
    });
    await new Promise((resolve) => setTimeout(resolve, 180));
  }
}

async function demoScan(onProgress: (progress: ScanProgress) => void) {
  const root = 'C:\\dev';
  for (let step = 1; step <= 4; step++) {
    onProgress({
      phase: 'Looking', root, current: `C:\\dev\\sample-${step}`, projectsFound: step,
      completed: step * 18, total: 0, rootIndex: 1, rootCount: 1, filesSeen: 0,
    });
    await new Promise((resolve) => setTimeout(resolve, 160));
  }
  for (let step = 1; step <= 5; step++) {
    onProgress({
      phase: 'Measuring', root, current: `${root}\\sample-${step}`, projectsFound: 4,
      completed: step, total: 5, rootIndex: 1, rootCount: 1, filesSeen: step * 1800,
    });
    await new Promise((resolve) => setTimeout(resolve, 180));
  }
}

function fullQuery(query: Partial<SweepQuery>): SweepQuery {
  return {
    dryRun: query.dryRun ?? true,
    trigger: query.trigger ?? 'manual',
    olderThanDays: query.olderThanDays ?? null,
    projectPath: query.projectPath ?? null,
    exclude: query.exclude ?? [],
    alwaysExclude: query.alwaysExclude ?? false,
    deleteMode: query.deleteMode ?? null,
    includeUnsafe: query.includeUnsafe ?? false,
  };
}

function createDemo() {
  const disk = {
    name: 'Local Disk (C:)',
    mount: 'C:\\',
    freeBytes: 64 * 1024 ** 3,
    totalBytes: 476 * 1024 ** 3,
    freeLabel: '64 GB',
    totalLabel: '476 GB',
    usedRatio: 0.82,
    reclaimableRatio: 0.04,
    freePercent: 13.4,
  };
  const gb = (n: number) => Math.round(n * 1024 ** 3);
  const mb = (n: number) => Math.round(n * 1024 ** 2);
  const projects: ProjectView[] = [
    row(1, 'hivemind-games', 'C:\\dev\\hivemind', 'Node · pnpm', 'node', 'JS', 'js', '12 days ago', 'node_modules ×6, .turbo', gb(8.2), 'stale', 'pnpm-lock.yaml', true),
    row(2, 'rust-browser', 'C:\\dev\\rust-browser', 'Rust', 'rust', 'RS', 'rs', '9 days ago', 'target', gb(5.1), 'stale', 'Cargo.lock', true),
    row(3, 'li-kick-app', 'C:\\work\\li-kick\\app', 'Ionic · Android', 'ionic', 'JS', 'js', '15 days ago', 'node_modules, android/app/build', gb(3.6), 'stale', 'package-lock.json', true),
    row(4, 'badgerbit-hud', 'C:\\fivem\\resources\\[badger]\\hud', 'FiveM · Node', 'fivem', 'JS', 'js', '44 days ago', 'node_modules', gb(1.5), 'stale', 'package-lock.json', true),
    row(5, 'selvr', 'C:\\dev\\selvr', 'Rust', 'rust', 'RS', 'rs', '2 hours ago', 'target', gb(9.4), 'active', 'Cargo.lock', true),
    row(6, 'courtos-web', 'C:\\dev\\courtos\\web', 'Node · Angular', 'node', 'JS', 'js', 'Yesterday', 'node_modules, .angular/cache', gb(1.9), 'active', 'package-lock.json', true),
    row(7, 'perminator', 'C:\\dev\\perminator', 'Node · NestJS', 'node', 'JS', 'js', '3 days ago', 'node_modules, dist', gb(2.4), 'protected', 'package-lock.json', true),
    row(8, 'buzzle-v1', 'C:\\dev\\buzzle-v1', 'Node · Angular', 'node', 'JS', 'js', '23 days ago', 'none', 0, 'cleaned', 'package-lock.json', true),
    row(9, 'old-portfolio', 'C:\\dev\\archive\\portfolio', 'Node · no lockfile', 'node', 'JS', 'js', '7 months ago', 'node_modules', mb(640), 'unsafe', null, false),
  ];
  const settings: Settings = {
    onboarded: true,
    confirmedRealSweep: true,
    thresholdDays: 7,
    dirtyCountsAsActive: true,
    scheduleTime: '23:00',
    headsUpMinutes: 15,
    onlyWhenIdle: true,
    idleMinutes: 10,
    onlyOnAc: true,
    skipFullscreen: true,
    catchUp: true,
    lowDiskEnabled: true,
    lowDiskPercent: 10,
    startAtLogin: true,
    deleteMode: 'permanent',
    skipWithoutLockfile: true,
    skipOpenProjects: true,
    respectMothballFiles: true,
    notifications: true,
    roots: [
      { path: 'C:\\dev', enabled: true, maxDepth: 6, thresholdDays: null },
      { path: 'C:\\work', enabled: true, maxDepth: 6, thresholdDays: 14 },
      { path: 'C:\\fivem\\resources', enabled: true, maxDepth: 6, thresholdDays: null },
    ],
    ecosystems: {},
    cacheToggles: { npm: false, pnpm: false, yarn: false, nuget: false, gradle: false, cargo: false, pip: false, uv: false, docker: false, wsl: false },
  };
  const rules = [
    rule('node', 'Node / JS', ['node_modules', '.next', '.nuxt', '.angular/cache', '.turbo'], ['dist', 'build'], true),
    rule('rust', 'Rust', ['target'], [], true),
    rule('dotnet', '.NET', ['bin', 'obj'], [], true),
    rule('python', 'Python', ['__pycache__', '.pytest_cache', '.mypy_cache', '.ruff_cache'], ['.venv', 'venv'], true),
    rule('gradle', 'Gradle / Android', ['build', '.gradle'], [], true),
    rule('maven', 'Maven', ['target'], [], true),
    rule('flutter', 'Flutter / Dart', ['.dart_tool', 'build'], [], true),
    rule('unity', 'Unity', ['Library', 'Temp', 'Obj', 'Logs'], ['Build'], false),
    rule('php', 'PHP / Composer', [], ['vendor'], false),
    rule('go', 'Go', [], [], true),
    rule('terraform', 'Terraform', ['.terraform'], [], true),
    rule('fivem', 'FiveM', [], ['dist'], true),
  ];
  for (const card of rules) {
    settings.ecosystems[card.id] = { enabled: card.enabled, optIn: [] };
  }
  let paused = false;
  let pauseLabel = '';
  let freed30 = 52.7 * 1024 ** 3;
  const store = {
    projects,
    settings,
    paused,
    pauseLabel,
    lastScanLabel: '4 min ago',
    dashboard(): Dashboard {
      const stale = projects.filter((project) => project.status === 'stale');
      const reclaim = stale.reduce((sum, project) => sum + project.sizeBytes, 0);
      return {
        reclaimableBytes: reclaim,
        reclaimableLabel: formatBytes(reclaim),
        freed30Bytes: freed30,
        freed30Label: formatBytes(freed30),
        freedAllBytes: 214 * 1024 ** 3,
        freedAllLabel: '214 GB',
        projectCount: projects.length,
        staleCount: stale.length,
        nextSweepLabel: store.paused ? 'Paused' : '11:00 PM',
        nextSweepSub: store.paused ? store.pauseLabel : 'Tonight',
        rootsLabel: settings.roots.map((root) => root.path).join(' · '),
        lastScanLabel: store.lastScanLabel,
        disk: { ...disk, reclaimableRatio: Math.min(0.2, reclaim / disk.totalBytes) },
        paused: store.paused,
        pauseLabel: store.pauseLabel,
        onboarded: settings.onboarded,
        confirmedRealSweep: settings.confirmedRealSweep,
        scheduleTime: settings.scheduleTime,
      };
    },
    tray(): TrayState {
      const dash = store.dashboard();
      return {
        watchingLabel: `Watching ${settings.roots.length} folders · ${projects.length} projects`,
        disk: dash.disk,
        reclaimableLabel: dash.reclaimableLabel,
        reclaimableBytes: dash.reclaimableBytes,
        staleProjects: dash.staleCount,
        nextSweepLabel: '11:00 PM',
        nextSweepPrefix: store.paused ? 'Paused' : 'Next sweep tonight at',
        paused: store.paused,
        pauseLabel: store.pauseLabel,
        recent: [
          { name: 'buzzle-v1', path: 'C:\\dev\\buzzle-v1', badge: 'JS', tone: 'js', detail: 'node_modules · .angular/cache', sizeLabel: '2.1 GB' },
          { name: 'selvr-playground', path: 'C:\\dev\\selvr', badge: 'RS', tone: 'rs', detail: 'target', sizeLabel: '6.8 GB' },
          { name: 'palbox-agent', path: 'C:\\dev\\palbox', badge: 'NET', tone: 'net', detail: 'bin · obj', sizeLabel: '940 MB' },
        ],
      };
    },
    preview(query: Partial<SweepQuery>): SweepPreview {
      const groups = projects
        .filter((project) => {
          if (query.projectPath && project.path !== query.projectPath) return false;
          if (project.status === 'stale') return true;
          return query.includeUnsafe && project.status === 'unsafe';
        })
        .map((project) => ({
          name: project.name,
          path: project.path,
          ageLabel: project.lastTouchedLabel.replace(' ago', ''),
          lockfile: project.lockfile,
          lockOk: project.lockOk,
          items: (() => {
            if (project.artifactSummary === 'none') return [];
            const parts = project.artifactSummary.split(', ');
            const each = Math.round(project.sizeBytes / parts.length);
            return parts.map((label) => ({
              label: label.includes('×') ? `${label.split('×')[0].trim()} (${label.split('×')[1]} workspaces)` : label,
              pattern: label.split(' ')[0],
              relPaths: [label.split(' ')[0]],
              sizeBytes: each,
              sizeLabel: formatBytes(each),
              checked: true,
            }));
          })(),
        }))
        .filter((group) => group.items.length);
      const selected = groups.reduce((sum, group) => sum + group.items.reduce((inner, item) => inner + (item.checked ? item.sizeBytes : 0), 0), 0);
      return {
        thresholdDays: settings.thresholdDays,
        deleteMode: settings.deleteMode,
        groups,
        totalBytes: selected,
        selectedBytes: selected,
        folderCount: groups.reduce((sum, group) => sum + group.items.length, 0),
        selectedFolders: groups.reduce((sum, group) => sum + group.items.length, 0),
      };
    },
    sweep(query: Partial<SweepQuery>): SweepOutcome {
      const excluded = new Set((query.exclude ?? []).map((item: ExcludeItem) => `${item.path}|${item.relPath}`));
      let bytes = 0;
      let cleaned = 0;
      for (const project of projects) {
        if (project.status !== 'stale' && !(query.includeUnsafe && project.status === 'unsafe')) continue;
        if (query.projectPath && project.path !== query.projectPath) continue;
        const skip = project.artifacts.some((artifact) => excluded.has(`${project.path}|${artifact.relPath}`));
        if (skip) continue;
        bytes += project.sizeBytes;
        cleaned += 1;
        if (!query.dryRun) {
          project.status = 'cleaned';
          project.action = 'rehydrate';
          project.sizeBytes = 0;
          project.sizeLabel = '0 B';
          project.artifactSummary = 'none';
        }
      }
      if (!query.dryRun) freed30 += bytes;
      settings.confirmedRealSweep = true;
      return {
        dryRun: !!query.dryRun,
        bytesFreed: bytes,
        bytesLabel: formatBytes(bytes),
        projectsCleaned: cleaned,
        errors: 0,
        skipped: 0,
        skippedSummary: 'None',
        message: query.dryRun ? `Would free ${formatBytes(bytes)} from ${cleaned} projects` : `Freed ${formatBytes(bytes)} from ${cleaned} projects`,
      };
    },
    history(days: number): HistoryView {
      const bars = demoBars(days);
      return {
        rangeDays: days,
        totalBytes: freed30,
        totalLabel: formatBytes(freed30),
        sweepCount: 11,
        allTimeBytes: 214 * 1024 ** 3,
        allTimeLabel: '214 GB',
        bars,
        sweeps: [
          { id: 5, whenLabel: 'Nov 20 · 11:00 PM', trigger: 'Scheduled', projectsLabel: '5 · hivemind-games, rust-browser +3', skippedLabel: '1 open in VS Code', freedLabel: '14.2 GB', freedBytes: gb(14.2) },
          { id: 4, whenLabel: 'Nov 18 · 2:14 PM', trigger: 'Manual (tray)', projectsLabel: '1 · fstats-old', skippedLabel: 'None', freedLabel: '1.5 GB', freedBytes: gb(1.5) },
          { id: 3, whenLabel: 'Nov 16 · 4:02 PM', trigger: 'Low disk (under 10% free)', projectsLabel: '3 · nutrisnap, orderfacts, +1', skippedLabel: 'None', freedLabel: '6.0 GB', freedBytes: gb(6) },
          { id: 2, whenLabel: 'Nov 13 · 11:00 PM', trigger: 'Scheduled', projectsLabel: '2 · keyspace, palbox', skippedLabel: '2 folders in use, retried', freedLabel: '3.1 GB', freedBytes: gb(3.1) },
          { id: 1, whenLabel: 'Nov 10 · 8:31 AM', trigger: 'Catch-up (PC was off)', projectsLabel: '2 · treehouse-site, jim-polls', skippedLabel: 'None', freedLabel: '2.4 GB', freedBytes: gb(2.4) },
        ],
      };
    },
    settingsView(): SettingsResponse {
      return {
        settings: structuredClone(settings),
        rules: rules.map((card) => ({
          ...card,
          enabled: settings.ecosystems[card.id]?.enabled ?? card.enabled,
          enabledOptIn: settings.ecosystems[card.id]?.optIn ?? [],
        })),
        roots: settings.roots.map((root) => ({
          ...root,
          projectCount: projects.filter((project) => project.path.toLowerCase().startsWith(root.path.toLowerCase())).length,
          thresholdLabel: `${projects.filter((project) => project.path.toLowerCase().startsWith(root.path.toLowerCase())).length} projects · ${root.thresholdDays ? root.thresholdDays + ' days' : 'default threshold'}`,
        })),
      };
    },
    caches(): CacheView[] {
      return [
        cache('npm', 'npm cache', 'Verifies the npm cache and drops unneeded entries.', 'npm cache verify', false),
        cache('pnpm', 'pnpm store', 'Removes packages no project links anymore.', 'pnpm store prune', false),
        cache('nuget', 'NuGet', 'Clears the global NuGet caches.', 'dotnet nuget locals all --clear', false),
        cache('cargo', 'Cargo', 'Runs cargo cache --autoclean when that tool is installed.', 'cargo cache --autoclean', false),
        cache('pip', 'pip', 'Purges the pip download cache.', 'pip cache purge', false),
        cache('uv', 'uv', 'Prunes unused uv cache entries.', 'uv cache prune', false),
        cache('docker', 'Docker', 'Removes unused images and build cache. This is off until you confirm it.', 'docker system prune -f', true),
        cache('wsl', 'Compact WSL disk', 'Marks WSL distros sparse so Windows can reclaim the VHDX. Deleting files inside WSL does not shrink it by itself.', 'wsl --manage <distro> --set-sparse true', true),
      ].map((item) => ({ ...item, enabled: !!settings.cacheToggles[item.id] }));
    },
  };
  return store;
}

function row(
  id: number, name: string, path: string, ecosystem: string, badgeId: string, badge: string, tone: string,
  age: string, summary: string, size: number, status: ProjectView['status'], lockfile: string | null, lockOk: boolean,
): ProjectView {
  const action = { stale: 'clean', active: 'protect', protected: 'unprotect', cleaned: 'rehydrate', unsafe: 'review' }[status];
  return {
    id, name, path, rootPath: path.split('\\').slice(0, 2).join('\\'), ecosystem,
    badges: [{ id: badgeId, label: badge, tone }],
    lastTouched: null, lastTouchedLabel: age, artifactSummary: summary, sizeBytes: size, sizeLabel: formatBytes(size),
    status, protected: status === 'protected', gitDirty: false, lockfile, lockOk,
    openReason: null,
    unsafeReason: status === 'unsafe' ? 'No lockfile — restoring this later would not be exact.' : null,
    action,
    artifacts: summary === 'none' ? [] : summary.split(', ').map((part) => ({
      relPath: part.split(' ')[0], pattern: part.split(' ')[0], ruleId: badgeId, sizeBytes: size, sizeLabel: formatBytes(size),
      kind: 'safe', deletable: status === 'stale', reason: status === 'unsafe' ? 'No lockfile' : null,
    })),
  };
}

function rule(id: string, name: string, safe: string[], optIn: string[], enabled: boolean) {
  return { id, name, safe, optIn, enabled, enabledOptIn: [] as string[] };
}

function cache(id: string, name: string, detail: string, command: string, dangerous: boolean): CacheView {
  return { id, name, detail, enabled: false, available: true, command, dangerous };
}

function demoBars(days: number) {
  const heights = [0.4, 1.1, 3.2, 0.2, 1.8, 5.4, 0.8, 0.3, 4.1, 1.6, 6.2, 2.1, 0.5, 2.8, 1.2, 6.1, 0.4, 0.9, 3.0, 5.8, 1.1, 14.2];
  const bars = [];
  const today = new Date(2026, 10, 20);
  for (let index = days - 1; index >= 0; index -= 1) {
    const date = new Date(today);
    date.setDate(today.getDate() - index);
    const sample = heights[(days - 1 - index) % heights.length];
    const label = index === days - 1 || index === 0 || index === Math.floor(days / 2)
      ? date.toLocaleDateString('en-US', { month: 'short', day: 'numeric' })
      : '';
    bars.push({ date: date.toISOString().slice(0, 10), label, bytes: sample * 1024 ** 3, bytesLabel: `${sample.toFixed(1)} GB` });
  }
  return bars;
}
