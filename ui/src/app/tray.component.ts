import { Component, inject, OnInit, signal } from '@angular/core';
import { ApiService } from './core/api.service';
import { TrayState } from './core/models';

@Component({
  selector: 'app-tray',
  template: `
    <div class="tray-body">
      @if (state(); as tray) {
        <div class="popover">
          <header>
            <svg viewBox="0 0 48 48" width="28" height="28" aria-hidden="true">
              <path d="M24 18c-7-9-16-8-18-2s6 12 14 9" fill="rgba(240,180,41,.2)" stroke="#f0b429" stroke-width="2"/>
              <path d="M24 18c7-9 16-8 18-2s-6 12-14 9" fill="rgba(240,180,41,.2)" stroke="#f0b429" stroke-width="2"/>
              <path d="M24 16v18" stroke="#f0b429" stroke-width="2"/>
            </svg>
            <div class="grow">
              <strong>Mothball</strong>
              <div class="faint small">{{ tray.watchingLabel }}</div>
            </div>
            <button class="icon-btn" title="Settings" (click)="open('settings')">⚙</button>
          </header>
          <div class="split" style="margin-top:14px"><span>{{ tray.disk.name }}</span><strong>{{ tray.disk.freeLabel }} free of {{ tray.disk.totalLabel }}</strong></div>
          <div class="meter">
            <span class="used" [style.width.%]="tray.disk.usedRatio * 100"></span>
            <span class="reclaim" [style.width.%]="tray.disk.reclaimableRatio * 100"></span>
          </div>
          <div class="legend"><span><i class="u"></i>Used</span><span><i class="r"></i>Reclaimable by Mothball</span></div>
          <div class="hero">
            <div class="hero-top">
              <div class="hero-num">{{ tray.reclaimableLabel }}</div>
              <div class="muted">in {{ tray.staleProjects }} stale projects</div>
            </div>
            <div style="margin-top:8px">{{ tray.nextSweepPrefix }} <strong>{{ tray.nextSweepLabel }}</strong></div>
            <div class="pair">
              <button class="btn primary" (click)="sweep()">Sweep now</button>
              <button class="btn" (click)="open('preview')">Preview</button>
            </div>
          </div>
          <div class="tiny" style="margin:14px 0 6px">Last night</div>
          @for (item of tray.recent; track item.path) {
            <div class="recent">
              <div class="badge {{ item.tone }}">{{ item.badge }}</div>
              <div class="grow"><strong>{{ item.name }}</strong><div class="faint small">{{ item.detail }}</div></div>
              <div>{{ item.sizeLabel }}</div>
              <button class="btn link" (click)="rehydrate(item.path)">Rehydrate</button>
            </div>
          }
          <div class="foot">
            <button class="btn ghost" (click)="paused.set(!paused())">{{ paused() ? 'Resume' : 'Pause' }}</button>
            <button class="btn" (click)="open('projects')">Open Mothball</button>
          </div>
          @if (paused()) {
            <div class="actions" style="margin-top:8px">
              <button class="btn" (click)="pause('1h')">For 1 hour</button>
              <button class="btn" (click)="pause('tomorrow')">Until tomorrow</button>
              <button class="btn" (click)="pause('forever')">Until I resume</button>
            </div>
          }
          @if (note()) { <div class="log">{{ note() }}</div> }
        </div>
      }
    </div>
  `,
})
export class TrayComponent implements OnInit {
  private readonly api = inject(ApiService);
  readonly state = signal<TrayState | null>(null);
  readonly paused = signal(false);
  readonly note = signal('');

  async ngOnInit() {
    const state = await this.api.tray();
    this.state.set(state);
    this.paused.set(state.paused);
  }

  async sweep() {
    const outcome = await this.api.sweep({ dryRun: false, trigger: 'tray' });
    this.note.set(outcome.message);
    this.state.set(await this.api.tray());
  }

  async rehydrate(path: string) {
    this.note.set('');
    await this.api.rehydrate(path, (line) => this.note.update((current) => `${current}${line}\n`));
  }

  async pause(until: string) {
    await this.api.setPause(until);
    this.paused.set(false);
    this.state.set(await this.api.tray());
  }

  async open(route: string) {
    if (!this.api.inTauri) {
      location.href = `/#/${route}`;
      return;
    }
    const mod = await import('@tauri-apps/api/core');
    await mod.invoke('open_main', { route });
  }
}
