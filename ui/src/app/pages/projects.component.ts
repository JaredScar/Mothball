import { Component, computed, inject, OnInit, signal } from '@angular/core';
import { ApiService } from '../core/api.service';
import { Dashboard, DeleteMode, PreviewGroup, ProjectStatus, ProjectView, ScanProgress, SweepPreview, SweepProgress, formatBytes } from '../core/models';
import { ScanStatusComponent } from '../scan-status.component';

@Component({
  selector: 'app-projects',
  imports: [ScanStatusComponent],
  template: `
    <div class="page-head">
      <div>
        <h1>Projects</h1>
        <p class="sub">{{ dash()?.rootsLabel }} · last scan {{ dash()?.lastScanLabel }}</p>
      </div>
      <div class="actions">
        <button class="btn" (click)="rescan()" [disabled]="busy()">{{ busy() ? 'Scanning…' : 'Rescan' }}</button>
        <button class="btn primary" (click)="openSweep()">Sweep stale now · {{ dash()?.reclaimableLabel }}</button>
      </div>
    </div>

    @if (message()) { <div class="banner">{{ message() }}</div> }
    @if (live(); as scan) {
      <div class="card scan-card">
        <app-scan-status [progress]="scan" [startedAt]="startedAt()" />
      </div>
    }

    <section class="stats">
      <article class="stat"><div class="label">Reclaimable now</div><div class="value amber">{{ dash()?.reclaimableLabel }}</div></article>
      <article class="stat"><div class="label">Freed · last 30 days</div><div class="value">{{ dash()?.freed30Label }}</div></article>
      <article class="stat"><div class="label">Projects</div><div class="value">{{ projects().length }} <span>{{ staleCount() }} stale</span></div></article>
      <article class="stat"><div class="label">Next sweep</div><div class="value">{{ dash()?.nextSweepLabel }} <span>{{ dash()?.nextSweepSub }}</span></div></article>
    </section>

    <div class="toolbar">
      <div class="chips">
        @for (chip of chips(); track chip.id) {
          <button class="chip" [class.on]="filter() === chip.id" (click)="filter.set(chip.id)">{{ chip.label }}</button>
        }
      </div>
      <input class="search" placeholder="Search projects or paths" [value]="query()" (input)="query.set(inputValue($event))" />
    </div>

    <div class="table-wrap">
      @if (!filtered().length) {
        <div class="empty">No projects in this view. Add a watched folder in Settings, then rescan.</div>
      } @else {
        <table>
          <thead>
            <tr><th>Project</th><th>Ecosystem</th><th>Last touched</th><th>Artifacts</th><th>Size</th><th>Status</th><th></th></tr>
          </thead>
          <tbody>
            @for (project of filtered(); track project.path) {
              <tr (click)="toggle(project.path)">
                <td><div class="name">{{ project.name }}</div><div class="path">{{ project.path }}</div></td>
                <td class="eco">{{ project.ecosystem }}</td>
                <td>{{ project.lastTouchedLabel }}</td>
                <td class="artifacts">{{ project.artifactSummary === 'none' ? '—' : project.artifactSummary }}</td>
                <td class="size">{{ project.sizeLabel }}</td>
                <td><span class="pill {{ project.status }}">{{ label(project.status) }}</span></td>
                <td class="right"><button class="btn" (click)="action(project, $event)">{{ actionLabel(project.action) }}</button></td>
              </tr>
              @if (open() === project.path) {
                <tr><td colspan="7" class="drawer">
                  @if (project.openReason) { <div class="banner">Skipped while sweeping: {{ project.openReason }}</div> }
                  @if (project.unsafeReason) { <div class="banner">{{ project.unsafeReason }}</div> }
                  @for (artifact of project.artifacts; track artifact.relPath) {
                    <div class="line"><span>{{ artifact.relPath }}</span><span>{{ artifact.sizeLabel }}{{ artifact.reason ? ' · ' + artifact.reason : '' }}</span></div>
                  } @empty { <div class="line"><span>No artifact folders right now.</span></div> }
                  @if (log() && logPath() === project.path) { <div class="log">{{ log() }}</div> }
                </td></tr>
              }
            }
          </tbody>
        </table>
      }
    </div>

    @if (sweep(); as preview) {
      <div class="overlay" (click)="running() ? stopSweep() : closeSweep()">
        <div class="modal" (click)="$event.stopPropagation()">
          <div class="split"><h2>Sweep preview</h2><button class="icon-btn" (click)="running() ? stopSweep() : closeSweep()">✕</button></div>
          <p class="help">These folders belong to projects untouched for {{ preview.thresholdDays }}+ days. Only regenerable folders are listed. Source code is never touched.</p>
          @for (group of preview.groups; track group.path) {
            <div class="group">
              <div class="group-head">
                <strong>{{ group.name }} <span class="muted">· {{ group.ageLabel }}</span></strong>
                <span class="faint">{{ group.lockfile || 'no lockfile' }} {{ group.lockOk ? '✓' : '' }}</span>
              </div>
              @for (item of group.items; track item.pattern) {
                <label class="item">
                  <span><input type="checkbox" [checked]="item.checked" [disabled]="running()" (change)="toggleItem(group, item.pattern)" /> {{ item.label }}</span>
                  <strong>{{ item.sizeLabel }}</strong>
                </label>
              }
            </div>
          }
          <div class="modal-foot">
            @if (clean(); as job) {
              <div class="clean-progress">
                <div class="split">
                  <strong>{{ job.project ? 'Deleting ' + job.project : 'Deleting' }}</strong>
                  <span class="days">{{ cleanEta() }}</span>
                </div>
                <div class="progress-track" [class.unknown]="cleanIndeterminate()" [class.known]="!cleanIndeterminate()">
                  <span class="fill" [style.width.%]="cleanIndeterminate() ? null : cleanPercent()"></span>
                </div>
                <div class="scan-path" [title]="job.path">{{ job.path || job.relPath }}</div>
                <div class="muted" style="margin-top:6px">{{ cleanDetail() }}</div>
              </div>
            }
            <div>
              <div class="schedule">
                <span class="muted">Delete mode</span>
                <select [value]="deleteMode()" (change)="deleteMode.set(selectValue($event))">
                  <option value="permanent">Permanent (recommended)</option>
                  <option value="trash">Recycle Bin</option>
                </select>
              </div>
              <label class="check"><input type="checkbox" [checked]="always()" (change)="always.set(!always())" /> Always exclude unchecked folders</label>
              <div class="faint">{{ selectedCount() }} of {{ folderCount() }} folders selected</div>
            </div>
            <div class="actions">
              <button class="btn" (click)="running() ? stopSweep() : closeSweep()">{{ stopping() ? 'Stopping…' : running() ? 'Stop' : 'Cancel' }}</button>
              <button class="btn primary" [disabled]="running()" (click)="confirmSweep()">{{ running() ? 'Cleaning…' : 'Free ' + selectedLabel() }}</button>
            </div>
          </div>
        </div>
      </div>
    }
  `,
})
export class ProjectsComponent implements OnInit {
  private readonly api = inject(ApiService);
  readonly dash = signal<Dashboard | null>(null);
  readonly projects = signal<ProjectView[]>([]);
  readonly filter = signal<ProjectStatus | 'all'>('all');
  readonly query = signal('');
  readonly open = signal<string | null>(null);
  readonly busy = signal(false);
  readonly startedAt = signal(0);
  readonly live = signal<ScanProgress | null>(null);
  readonly running = signal(false);
  readonly stopping = signal(false);
  readonly clean = signal<SweepProgress | null>(null);
  readonly cleanStarted = signal(0);
  readonly message = signal('');
  readonly sweep = signal<SweepPreview | null>(null);
  readonly deleteMode = signal<DeleteMode>('permanent');
  readonly always = signal(false);
  readonly log = signal('');
  readonly logPath = signal('');

  readonly filtered = computed(() => {
    const q = this.query().trim().toLowerCase();
    return this.projects().filter((project) => {
      if (this.filter() !== 'all' && project.status !== this.filter()) return false;
      if (!q) return true;
      return project.name.toLowerCase().includes(q) || project.path.toLowerCase().includes(q);
    });
  });
  readonly staleCount = computed(() => this.projects().filter((project) => project.status === 'stale').length);
  readonly chips = computed(() => {
    const all = this.projects();
    const count = (status: ProjectStatus) => all.filter((project) => project.status === status).length;
    return [
      { id: 'all' as const, label: `All ${all.length}` },
      { id: 'stale' as const, label: `Stale ${count('stale')}` },
      { id: 'active' as const, label: `Active ${count('active')}` },
      { id: 'protected' as const, label: `Protected ${count('protected')}` },
      { id: 'cleaned' as const, label: `Cleaned ${count('cleaned')}` },
      { id: 'unsafe' as const, label: `Unsafe ${count('unsafe')}` },
    ];
  });
  readonly selectedLabel = computed(() => formatBytes(this.selectedBytes()));
  readonly selectedCount = computed(() => this.sweep()?.groups.reduce((sum, group) => sum + group.items.filter((item) => item.checked).length, 0) ?? 0);
  readonly folderCount = computed(() => this.sweep()?.groups.reduce((sum, group) => sum + group.items.length, 0) ?? 0);

  async ngOnInit() {
    await this.reload();
    if (this.api.inTauri) {
      void this.api.on('data-changed', () => {
        this.live.set(null);
        this.startedAt.set(0);
        this.busy.set(false);
        void this.reload();
      });
      void this.api.on<ScanProgress>('scan-progress', (progress) => {
        if (!this.startedAt()) this.startedAt.set(Date.now());
        this.busy.set(true);
        this.live.set(progress);
      });
    }
  }

  async reload() {
    const [dash, projects, settings] = await Promise.all([this.api.dashboard(), this.api.projects(), this.api.settings()]);
    this.dash.set(dash);
    this.projects.set(projects);
    this.deleteMode.set(settings.settings.deleteMode);
  }

  async rescan() {
    this.busy.set(true);
    this.message.set('');
    this.startedAt.set(Date.now());
    this.live.set({
      phase: 'Looking', root: '', current: 'Preparing the scan…', projectsFound: 0,
      completed: 0, total: 0, rootIndex: 1, rootCount: 1, filesSeen: 0,
    });
    try {
      await this.api.scan((progress) => this.live.set(progress));
      await this.reload();
    } catch (error) {
      this.message.set(String(error));
    } finally {
      this.live.set(null);
      this.startedAt.set(0);
      this.busy.set(false);
    }
  }

  toggle(path: string) { this.open.set(this.open() === path ? null : path); }

  async openSweep(project?: ProjectView) {
    const preview = await this.api.preview({
      dryRun: true,
      projectPath: project?.path ?? null,
      includeUnsafe: project?.status === 'unsafe',
    });
    this.sweep.set(preview);
  }

  closeSweep() {
    if (this.running()) {
      this.stopSweep();
      return;
    }
    this.sweep.set(null);
  }

  stopSweep() {
    if (!this.running() || this.stopping()) return;
    this.stopping.set(true);
    void this.api.cancelSweep();
  }

  cleanIndeterminate() {
    const job = this.clean();
    return !job || job.bytesFreed === 0;
  }

  cleanPercent() {
    const job = this.clean();
    if (!job?.bytesTotal) return 0;
    return Math.min(99, Math.round((job.bytesFreed / job.bytesTotal) * 100));
  }

  cleanEta() {
    const job = this.clean();
    if (!job || job.bytesFreed === 0 || !job.bytesTotal) return 'Estimating time remaining…';
    const elapsed = Date.now() - this.cleanStarted();
    const remaining = (elapsed / job.bytesFreed) * (job.bytesTotal - job.bytesFreed);
    const left = job.bytesFreed >= job.bytesTotal ? 'Finishing up' : remaining < 10_000 ? 'A few seconds left' : `About ${formatDuration(remaining)} left`;
    return `${this.cleanPercent()}% · ${left}`;
  }

  cleanDetail() {
    const job = this.clean();
    if (!job) return '';
    const elapsed = formatDuration(Date.now() - this.cleanStarted());
    const freed = formatBytes(job.bytesFreed);
    const total = formatBytes(job.bytesTotal);
    return `${job.index} of ${job.total} folders · ${freed} of ${total} · ${job.filesRemoved.toLocaleString()} files removed · ${elapsed} elapsed`;
  }

  toggleItem(group: PreviewGroup, pattern: string) {
    const preview = this.sweep();
    if (!preview) return;
    this.sweep.set({
      ...preview,
      groups: preview.groups.map((candidate) => candidate.path !== group.path ? candidate : {
        ...candidate,
        items: candidate.items.map((item) => item.pattern === pattern ? { ...item, checked: !item.checked } : item),
      }),
    });
  }

  async confirmSweep() {
    const preview = this.sweep();
    if (!preview) return;
    this.running.set(true);
    this.stopping.set(false);
    this.cleanStarted.set(Date.now());
    this.clean.set({
      project: '', path: 'Starting the clean…', relPath: '', index: 0, total: preview.selectedFolders,
      bytesFreed: 0, bytesTotal: preview.selectedBytes, filesRemoved: 0,
    });
    const exclude = preview.groups.flatMap((group) => group.items.filter((item) => !item.checked).flatMap((item) => item.relPaths.map((relPath) => ({ path: group.path, relPath }))));
    try {
      const outcome = await this.api.sweep({
        dryRun: false,
        trigger: 'manual',
        exclude,
        alwaysExclude: this.always(),
        deleteMode: this.deleteMode(),
        includeUnsafe: preview.groups.some((group) => this.projects().find((project) => project.path === group.path)?.status === 'unsafe'),
      }, (progress) => this.clean.set(progress));
      this.message.set(outcome.message);
      this.sweep.set(null);
      await this.reload();
    } catch (error) {
      this.message.set(String(error));
    } finally {
      this.running.set(false);
      this.stopping.set(false);
      this.clean.set(null);
    }
  }

  async action(project: ProjectView, event: Event) {
    event.stopPropagation();
    if (project.action === 'clean' || project.action === 'review') {
      await this.openSweep(project);
      return;
    }
    if (project.action === 'protect' || project.action === 'unprotect') {
      await this.api.setProtected(project.path, project.action === 'protect');
      await this.reload();
      return;
    }
    if (project.action === 'rehydrate') {
      this.open.set(project.path);
      this.logPath.set(project.path);
      this.log.set('');
      try {
        await this.api.rehydrate(project.path, (line) => this.log.update((current) => `${current}${line}\n`));
        await this.reload();
      } catch (error) {
        this.log.update((current) => `${current}${error}\n`);
      }
    }
  }

  label(status: ProjectStatus) {
    return status[0].toUpperCase() + status.slice(1);
  }

  actionLabel(action: string) {
    const labels: Record<string, string> = {
      clean: 'Clean now', protect: 'Protect', unprotect: 'Unprotect', rehydrate: 'Rehydrate', review: 'Review',
    };
    return labels[action] ?? action;
  }

  inputValue(event: Event) { return (event.target as HTMLInputElement).value; }
  selectValue(event: Event) { return (event.target as HTMLSelectElement).value as DeleteMode; }
  private selectedBytes() {
    return this.sweep()?.groups.reduce((sum, group) => sum + group.items.filter((item) => item.checked).reduce((inner, item) => inner + item.sizeBytes, 0), 0) ?? 0;
  }
}

function formatDuration(ms: number): string {
  const sec = Math.max(0, Math.round(ms / 1000));
  if (sec < 60) return sec === 1 ? '1 sec' : `${sec} sec`;
  const min = Math.floor(sec / 60);
  const rem = sec % 60;
  if (min < 60) return rem ? `${min} min ${rem} sec` : `${min} min`;
  const hours = Math.floor(min / 60);
  const minutes = min % 60;
  return minutes ? `${hours} hr ${minutes} min` : `${hours} hr`;
}
