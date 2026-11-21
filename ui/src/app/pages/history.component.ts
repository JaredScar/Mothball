import { Component, inject, OnInit, signal } from '@angular/core';
import { ApiService } from '../core/api.service';
import { HistoryView } from '../core/models';

@Component({
  selector: 'app-history',
  template: `
    <div class="page-head">
      <div>
        <h1>History</h1>
        <p class="sub">Every folder Mothball deleted, and why</p>
      </div>
      <button class="btn" (click)="exportCsv()">Export CSV</button>
    </div>
    @if (history(); as view) {
      <section class="card chart-card">
        <div class="chart-head">
          <div>
            <div class="muted">Space freed · last {{ view.rangeDays }} days</div>
            <div class="value" style="font-size:32px;font-weight:720">{{ view.totalLabel }} <span class="muted" style="font-size:14px;font-weight:500">· {{ view.sweepCount }} sweeps · {{ view.allTimeLabel }} all time</span></div>
          </div>
          <div class="chips">
            @for (days of [7, 30, 90]; track days) {
              <button class="chip" [class.on]="view.rangeDays === days" (click)="load(days)">{{ days }} days</button>
            }
          </div>
        </div>
        <div class="chart">
          @for (bar of view.bars; track bar.date) {
            <div class="col" [title]="bar.bytesLabel + ' · ' + bar.date">
              <div class="bar" [style.height.%]="height(bar.bytes)"></div>
              <div class="tick">{{ bar.label }}</div>
            </div>
          }
        </div>
      </section>
      <div class="table-wrap" style="margin-top:14px">
        <table>
          <thead><tr><th>When</th><th>Trigger</th><th>Projects</th><th>Skipped</th><th class="right">Freed</th></tr></thead>
          <tbody>
            @for (sweep of view.sweeps; track sweep.id) {
              <tr>
                <td>{{ sweep.whenLabel }}</td>
                <td>{{ sweep.trigger }}</td>
                <td>{{ sweep.projectsLabel }}</td>
                <td class="muted">{{ sweep.skippedLabel }}</td>
                <td class="right size">{{ sweep.freedLabel }}</td>
              </tr>
            } @empty { <tr><td colspan="5" class="empty">No sweeps yet.</td></tr> }
          </tbody>
        </table>
      </div>
    }
  `,
})
export class HistoryComponent implements OnInit {
  private readonly api = inject(ApiService);
  readonly history = signal<HistoryView | null>(null);
  private max = 1;

  async ngOnInit() { await this.load(30); }

  async load(days: number) {
    const view = await this.api.history(days);
    this.max = Math.max(1, ...view.bars.map((bar) => bar.bytes));
    this.history.set(view);
  }

  height(bytes: number) { return Math.max(bytes ? 4 : 0, (bytes / this.max) * 100); }

  exportCsv() { void this.api.exportHistory(); }
}
