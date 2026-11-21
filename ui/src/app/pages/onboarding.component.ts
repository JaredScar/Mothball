import { Component, inject, OnInit, signal } from '@angular/core';
import { Router } from '@angular/router';
import { ApiService } from '../core/api.service';
import { ScanProgress, SuggestedRoot } from '../core/models';
import { ScanStatusComponent } from '../scan-status.component';

@Component({
  selector: 'app-onboarding',
  imports: [ScanStatusComponent],
  template: `
    <div class="onboard">
      <div class="panel card">
        @switch (step()) {
          @case (1) {
            <h1>Mothball</h1>
            <p>Mothball cleans regenerable dev folders from projects you haven't touched in a while. Your code is never deleted.</p>
            <p class="help">node_modules, target, bin/obj, and the other folders a lockfile can rebuild. Nothing else.</p>
            <button class="btn primary" (click)="step.set(2)">Choose folders</button>
          }
          @case (2) {
            <h2>Where should Mothball look?</h2>
            <p class="help">Pick the folders that hold your repos. Links are never followed.</p>
            <div class="suggest">
              @for (root of suggestions(); track root.path) {
                <button [class.on]="chosen().includes(root.path)" (click)="toggle(root.path)">
                  <strong>{{ root.path }}</strong>
                  <div class="faint">{{ root.reason }}</div>
                </button>
              }
            </div>
            <div class="actions">
              <button class="btn" (click)="browse()">Add another folder</button>
              <button class="btn primary" [disabled]="!chosen().length" (click)="scan()">Scan</button>
            </div>
          }
          @case (3) {
            <h2>Scanning</h2>
            <app-scan-status [progress]="live()" [startedAt]="startedAt()" />
          }
          @case (4) {
            <h2>When should a project be mothballed?</h2>
            <p class="help">Default is 7 days, every night at 11:00 PM, and only while you're idle and on power.</p>
            <div class="split"><span>Untouched for</span><strong class="days">{{ days() }} days</strong></div>
            <input class="slider" type="range" min="1" max="90" [value]="days()" (input)="days.set(numberValue($event))" />
            <div class="actions" style="margin-top:16px">
              <button class="btn primary" (click)="finish()">Finish setup</button>
            </div>
          }
        }
        @if (error()) { <div class="banner">{{ error() }}</div> }
      </div>
    </div>
  `,
})
export class OnboardingComponent implements OnInit {
  private readonly api = inject(ApiService);
  private readonly router = inject(Router);
  readonly step = signal(1);
  readonly suggestions = signal<SuggestedRoot[]>([]);
  readonly chosen = signal<string[]>([]);
  readonly startedAt = signal(0);
  readonly live = signal<ScanProgress>({
    phase: 'Looking', root: '', current: 'Preparing the scan…', projectsFound: 0,
    completed: 0, total: 0, rootIndex: 1, rootCount: 1, filesSeen: 0,
  });
  readonly days = signal(7);
  readonly error = signal('');

  async ngOnInit() {
    this.suggestions.set(await this.api.suggestRoots());
  }

  toggle(path: string) {
    const current = this.chosen();
    this.chosen.set(current.includes(path) ? current.filter((item) => item !== path) : [...current, path]);
  }

  async browse() {
    const picked = await this.api.pickFolder();
    const path = picked || (this.api.inTauri ? '' : prompt('Folder to watch') || '');
    if (!path) return;
    if (!this.suggestions().some((root) => root.path === path)) {
      this.suggestions.update((rows) => [...rows, { path, reason: 'Added by you', gitRepos: 0 }]);
    }
    this.toggle(path);
  }

  async scan() {
    this.step.set(3);
    this.startedAt.set(Date.now());
    try {
      for (const path of this.chosen()) await this.api.addRoot(path);
      await this.api.scan((progress) => this.live.set(progress));
      this.step.set(4);
    } catch (error) {
      this.error.set(String(error));
    }
  }

  async finish() {
    const settings = (await this.api.settings()).settings;
    settings.onboarded = true;
    settings.thresholdDays = this.days();
    settings.scheduleTime = '23:00';
    await this.api.saveSettings(settings);
    await this.router.navigateByUrl('/projects');
  }

  numberValue(event: Event) { return Number((event.target as HTMLInputElement).value); }
}
