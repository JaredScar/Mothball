import { Component, inject, OnInit, signal } from '@angular/core';
import { RouterLink, RouterLinkActive, RouterOutlet } from '@angular/router';
import { ApiService } from './core/api.service';
import { Dashboard } from './core/models';

@Component({
  selector: 'app-shell',
  imports: [RouterLink, RouterLinkActive, RouterOutlet],
  template: `
    <div class="app">
      <aside class="sidebar">
        <div class="brand">
          <svg viewBox="0 0 48 48" aria-hidden="true">
            <path d="M24 18c-7-9-16-8-18-2s6 12 14 9" fill="rgba(240,180,41,.18)" stroke="#f0b429" stroke-width="2"/>
            <path d="M24 18c7-9 16-8 18-2s-6 12-14 9" fill="rgba(240,180,41,.18)" stroke="#f0b429" stroke-width="2"/>
            <path d="M24 24c-6 2-11 8-8 12 4 2 9-2 10-7" fill="none" stroke="#f0b429" stroke-width="2"/>
            <path d="M24 24c6 2 11 8 8 12-4 2-9-2-10-7" fill="none" stroke="#f0b429" stroke-width="2"/>
            <path d="M24 16v18" stroke="#f0b429" stroke-width="2" stroke-linecap="round"/>
            <path d="M22 15c-2-4-1-7 1-9M26 15c2-4 1-7-1-9" fill="none" stroke="#f0b429" stroke-width="1.7" stroke-linecap="round"/>
          </svg>
          Mothball
        </div>
        <nav class="nav">
          <a routerLink="/projects" routerLinkActive="active"><svg viewBox="0 0 24 24"><path d="M4 7h7l2 2h7v9a2 2 0 0 1-2 2H4z"/><path d="M4 7V5h6l2 2"/></svg>Projects</a>
          <a routerLink="/preview" routerLinkActive="active"><svg viewBox="0 0 24 24"><path d="M4 7h16M4 12h16M4 17h10"/></svg>Sweep preview</a>
          <a routerLink="/history" routerLinkActive="active"><svg viewBox="0 0 24 24"><path d="M4 19V5M4 19h16"/><path d="M8 15v-3M12 15V8M16 15v-5"/></svg>History</a>
          <a routerLink="/caches" routerLinkActive="active"><svg viewBox="0 0 24 24"><rect x="4" y="4" width="16" height="6" rx="1"/><rect x="4" y="14" width="16" height="6" rx="1"/></svg>Global caches</a>
          <a routerLink="/settings" routerLinkActive="active"><svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="3"/><path d="M12 3v2M12 19v2M3 12h2M19 12h2M5.6 5.6l1.4 1.4M17 17l1.4 1.4M18.4 5.6 17 7M7 17l-1.4 1.4"/></svg>Settings</a>
        </nav>
        @if (dash(); as diskDash) {
          <div class="disk-card">
            <div class="split small"><span>{{ diskDash.disk.name }}</span></div>
            <div class="meter">
              <span class="used" [style.width.%]="diskDash.disk.usedRatio * 100"></span>
              <span class="reclaim" [style.width.%]="diskDash.disk.reclaimableRatio * 100"></span>
            </div>
            <div class="small" style="margin-top:8px"><b>{{ diskDash.disk.freeLabel }} free</b> · {{ diskDash.disk.totalLabel }}</div>
          </div>
        }
      </aside>
      <main class="main">
        <router-outlet />
      </main>
    </div>
  `,
})
export class ShellComponent implements OnInit {
  private readonly api = inject(ApiService);
  readonly dash = signal<Dashboard | null>(null);

  async ngOnInit() {
    this.dash.set(await this.api.dashboard());
    if (this.api.inTauri) {
      void this.api.on('data-changed', () => { void this.api.dashboard().then((dash) => this.dash.set(dash)); });
    }
  }
}
