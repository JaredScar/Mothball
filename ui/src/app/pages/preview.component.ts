import { Component, inject, OnInit, signal } from '@angular/core';
import { Router } from '@angular/router';
import { ApiService } from '../core/api.service';
import { SweepPreview } from '../core/models';

@Component({
  selector: 'app-preview',
  template: `
    <div class="page-head">
      <div>
        <h1>Sweep preview</h1>
        <p class="sub">Exactly what the next sweep would delete. Source code is never on this list.</p>
      </div>
      <button class="btn primary" (click)="go()">Review in the cleaner</button>
    </div>
    @if (preview(); as view) {
      @if (!view.groups.length) {
        <div class="card empty">Nothing is stale enough to clean.</div>
      }
      @for (group of view.groups; track group.path) {
        <section class="card">
          <div class="split"><strong>{{ group.name }}</strong><span class="muted">{{ group.ageLabel }} · {{ group.lockfile || 'no lockfile' }}</span></div>
          @for (item of group.items; track item.pattern) {
            <div class="root-row"><span>{{ item.label }}</span><strong>{{ item.sizeLabel }}</strong></div>
          }
        </section>
      }
    }
  `,
})
export class PreviewComponent implements OnInit {
  private readonly api = inject(ApiService);
  private readonly router = inject(Router);
  readonly preview = signal<SweepPreview | null>(null);
  async ngOnInit() { this.preview.set(await this.api.preview({ dryRun: true })); }
  go() { void this.router.navigateByUrl('/projects'); }
}
