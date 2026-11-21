import { Component, inject, OnInit, signal } from '@angular/core';
import { ApiService } from './core/api.service';

@Component({
  selector: 'app-toast',
  template: `
    <div class="toast-body">
      @if (kind() === 'done') {
        <div class="toast">
          <div class="split">
            <div class="ok">✓</div>
            <div class="grow">
              <strong>{{ title() }}</strong>
              <div class="muted">{{ body() }} <button class="btn link" (click)="openHistory()">View details</button></div>
            </div>
          </div>
        </div>
      } @else {
        <div class="toast">
          <div class="faint small">Mothball · now</div>
          <h2 style="margin-top:8px">{{ title() }}</h2>
          <p class="help">{{ body() }}</p>
          <div class="btns">
            <button class="btn" (click)="act('preview')">Preview</button>
            <button class="btn" (click)="act('snooze')">Snooze 1h</button>
            <button class="btn" (click)="act('skip')">Skip tonight</button>
          </div>
        </div>
      }
    </div>
  `,
})
export class ToastComponent implements OnInit {
  private readonly api = inject(ApiService);
  readonly kind = signal('heads-up');
  readonly title = signal('Sweep starts at 11:00 PM');
  readonly body = signal('Stale projects will be cleaned. Only regenerable folders are deleted.');

  async ngOnInit() {
    const toast = await this.api.toast();
    this.kind.set(toast.kind || 'heads-up');
    this.title.set(toast.title);
    this.body.set(toast.body);
    if (this.kind() === 'done') {
      setTimeout(() => { void this.act('close'); }, 8000);
    }
  }

  async act(action: string) {
    if (action === 'snooze') await this.api.setPause('1h');
    if (action === 'skip') await this.api.setPause('tonight');
    if (action === 'preview') await this.open('preview');
    if (this.api.inTauri) {
      const mod = await import('@tauri-apps/api/window');
      await mod.getCurrentWindow().hide();
    }
  }

  async openHistory() { await this.open('history'); }

  private async open(route: string) {
    if (!this.api.inTauri) {
      location.href = `/#/${route}`;
      return;
    }
    const mod = await import('@tauri-apps/api/core');
    await mod.invoke('open_main', { route });
  }
}
