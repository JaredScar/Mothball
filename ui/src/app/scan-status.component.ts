import { Component, input } from '@angular/core';
import { ScanProgress } from './core/models';

@Component({
  selector: 'app-scan-status',
  template: `
    @if (progress(); as scan) {
      @let view = labels();
      <div class="split">
        <strong>{{ view.title }}</strong>
        <span class="days">{{ view.eta }}</span>
      </div>
      <div class="progress-track" [class.unknown]="view.indeterminate" [class.known]="!view.indeterminate && scan.completed < scan.total">
        <span class="fill" [style.width.%]="view.indeterminate ? null : view.percent"></span>
      </div>
      <div class="scan-path" [title]="scan.current">{{ scan.current || scan.root }}</div>
      <div class="muted" style="margin-top:6px">{{ view.detail }}</div>
    }
  `,
})
export class ScanStatusComponent {
  readonly progress = input.required<ScanProgress>();
  readonly startedAt = input(0);

  private clockKey = '';
  private clockAt = 0;

  labels() {
    return this.describe(this.progress());
  }

  private describe(scan: ScanProgress) {
    const key = `${scan.phase}:${scan.root}:${scan.total}`;
    if (key !== this.clockKey) {
      this.clockKey = key;
      this.clockAt = Date.now();
    }
    const elapsed = this.startedAt() ? Date.now() - this.startedAt() : 0;
    const indeterminate = scan.phase !== 'Measuring' || scan.total === 0 || scan.completed === 0;
    const percent = !indeterminate && scan.total ? Math.min(100, Math.round((scan.completed / scan.total) * 100)) : 0;
    let eta = 'Estimating time remaining…';
    if (!indeterminate) {
      const spent = Date.now() - this.clockAt;
      const remaining = (spent / scan.completed) * (scan.total - scan.completed);
      const left = scan.completed >= scan.total ? 'Finishing up' : remaining < 10_000 ? 'A few seconds left' : `About ${formatDuration(remaining)} left`;
      eta = `${percent}% · ${left}`;
    }
    const roots = scan.rootCount > 1 ? `Folder ${scan.rootIndex} of ${scan.rootCount} · ` : '';
    const elapsedLabel = `${formatDuration(elapsed)} elapsed`;
    const detail = scan.phase === 'Measuring'
      ? scan.completed === 0
        ? `${roots}Sizing this project · ${scan.filesSeen.toLocaleString()} files · ${elapsedLabel}`
        : `${roots}${scan.completed} of ${scan.total} projects · ${scan.filesSeen.toLocaleString()} files sized · ${elapsedLabel}`
      : `${roots}${scan.projectsFound === 1 ? '1 project' : `${scan.projectsFound} projects`} · ${scan.completed.toLocaleString()} folders checked · ${elapsedLabel}`;
    const title = scan.phase === 'Measuring' ? 'Measuring folder sizes' : 'Looking for projects';
    return { title, eta, percent, detail, indeterminate };
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
