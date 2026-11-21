import { Component, inject, OnInit, signal } from '@angular/core';
import { ApiService } from '../core/api.service';
import { CacheView } from '../core/models';

@Component({
  selector: 'app-caches',
  template: `
    <div class="page-head">
      <div>
        <h1>Global caches</h1>
        <p class="sub">Tool caches live outside any one project. Each one uses that tool's own prune command.</p>
      </div>
    </div>
    <section class="card">
      @for (cache of caches(); track cache.id) {
        <div class="cache-row">
          <label class="check" style="margin:0">
            <input type="checkbox" [checked]="cache.enabled" (change)="toggle(cache)" />
            <span>
              <strong>{{ cache.name }}</strong>
              <div class="faint">{{ cache.detail }}</div>
              <div class="faint small" style="margin-top:4px">{{ cache.command }}</div>
            </span>
          </label>
          <button class="btn" [disabled]="!cache.available || running() === cache.id" (click)="prune(cache)">
            {{ running() === cache.id ? 'Running…' : cache.dangerous ? 'Prune…' : 'Prune' }}
          </button>
        </div>
      }
      @if (output()) { <div class="log">{{ output() }}</div> }
    </section>
  `,
})
export class CachesComponent implements OnInit {
  private readonly api = inject(ApiService);
  readonly caches = signal<CacheView[]>([]);
  readonly running = signal('');
  readonly output = signal('');

  async ngOnInit() { this.caches.set(await this.api.caches()); }

  async toggle(cache: CacheView) {
    const settings = (await this.api.settings()).settings;
    settings.cacheToggles[cache.id] = !cache.enabled;
    await this.api.saveSettings(settings);
    this.caches.set(await this.api.caches());
  }

  async prune(cache: CacheView) {
    if (cache.dangerous && !confirm(`Run ${cache.command}? This cannot be undone.`)) return;
    this.running.set(cache.id);
    this.output.set('');
    try {
      this.output.set(await this.api.pruneCache(cache.id, true));
    } catch (error) {
      this.output.set(String(error));
    } finally {
      this.running.set('');
    }
  }
}
