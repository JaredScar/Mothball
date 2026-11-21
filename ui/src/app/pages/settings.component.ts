import { Component, inject, OnInit, signal } from '@angular/core';
import { ApiService } from '../core/api.service';
import { RuleCard, SettingsResponse } from '../core/models';

@Component({
  selector: 'app-settings',
  template: `
    @if (view(); as data) {
      <div class="page-head"><div><h1>Settings</h1></div></div>

      <section class="card">
        <div class="split">
          <div>
            <h2>Watched folders</h2>
            <p class="help">Mothball looks for projects inside these folders (max depth 6). Links and junctions are never followed.</p>
          </div>
          <button class="btn" (click)="addFolder()">Add folder</button>
        </div>
        @for (root of data.roots; track root.path) {
          <div class="root-row">
            <div>
              <div>{{ root.path }}</div>
            </div>
            <div class="muted">{{ root.thresholdLabel }} <button class="icon-btn" (click)="remove(root.path)">✕</button></div>
          </div>
        } @empty { <div class="empty">No folders yet.</div> }
      </section>

      <section class="card">
        <div class="split"><h2>When is a project stale?</h2><span class="days">{{ data.settings.thresholdDays }} days</span></div>
        <p class="help">Untouched for at least</p>
        <input class="slider" type="range" min="1" max="90" [value]="data.settings.thresholdDays" (input)="setDays($event)" />
        <div class="slider-labels"><span>1 day</span><span>30 days</span><span>90 days</span></div>
        <p class="help">“Touched” means a source file changed, a commit was made, or files were staged. Opening a folder doesn't count.</p>
        <label class="check"><input type="checkbox" [checked]="data.settings.dirtyCountsAsActive" (change)="patch({ dirtyCountsAsActive: !data.settings.dirtyCountsAsActive })" /> Treat projects with uncommitted git changes as active</label>
      </section>

      <section class="card">
        <h2>Schedule</h2>
        <div class="schedule">
          <span>Sweep every day at</span>
          <input type="time" [value]="data.settings.scheduleTime" (change)="patch({ scheduleTime: timeValue($event) })" />
          <span>Heads-up</span>
          <select [value]="'' + data.settings.headsUpMinutes" (change)="patch({ headsUpMinutes: numberValue($event) })">
            <option value="5">5 min before</option>
            <option value="15">15 min before</option>
            <option value="30">30 min before</option>
            <option value="60">60 min before</option>
          </select>
        </div>
        <div class="checks">
          <label class="check"><input type="checkbox" [checked]="data.settings.onlyWhenIdle" (change)="patch({ onlyWhenIdle: !data.settings.onlyWhenIdle })" /> Only when idle for {{ data.settings.idleMinutes }}+ minutes</label>
          <label class="check"><input type="checkbox" [checked]="data.settings.onlyOnAc" (change)="patch({ onlyOnAc: !data.settings.onlyOnAc })" /> Only on AC power</label>
          <label class="check"><input type="checkbox" [checked]="data.settings.skipFullscreen" (change)="patch({ skipFullscreen: !data.settings.skipFullscreen })" /> Not while a full-screen app or game is running</label>
          <label class="check"><input type="checkbox" [checked]="data.settings.catchUp" (change)="patch({ catchUp: !data.settings.catchUp })" /> Catch up after login if a sweep was missed</label>
          <label class="check"><input type="checkbox" [checked]="data.settings.lowDiskEnabled" (change)="patch({ lowDiskEnabled: !data.settings.lowDiskEnabled })" /> Sweep early when C: is below {{ data.settings.lowDiskPercent }}% free</label>
          <label class="check"><input type="checkbox" [checked]="data.settings.startAtLogin" (change)="patch({ startAtLogin: !data.settings.startAtLogin })" /> Start Mothball when I sign in</label>
        </div>
      </section>

      <section class="card">
        <h2>What gets cleaned</h2>
        <p class="help">Amber items are opt-in: some repos keep hand-made files in them. Click an amber name to include it.</p>
        <div class="rule-grid">
          @for (rule of data.rules; track rule.id) {
            <div class="rule" [class.off]="!rule.enabled">
              <label class="title"><input type="checkbox" [checked]="rule.enabled" (change)="toggleRule(rule)" /> {{ rule.name }}</label>
              <div class="arts">{{ rule.safe.join('  ') }}</div>
              @if (rule.optIn.length) {
                <div class="opt">opt-in:
                  @for (name of rule.optIn; track name) {
                    <button [class.on]="rule.enabledOptIn.includes(name)" (click)="toggleOpt(rule, name)">{{ name }}</button>
                  }
                </div>
              }
            </div>
          }
        </div>
      </section>

      <section class="card">
        <h2>Safety</h2>
        <label class="check"><input type="checkbox" [checked]="data.settings.skipWithoutLockfile" (change)="patch({ skipWithoutLockfile: !data.settings.skipWithoutLockfile })" /> Skip projects without a lockfile (they can't be restored exactly)</label>
        <label class="check"><input type="checkbox" [checked]="data.settings.skipOpenProjects" (change)="patch({ skipOpenProjects: !data.settings.skipOpenProjects })" /> Skip projects open in VS Code, Cursor or JetBrains, or with files in use</label>
        <label class="check"><input type="checkbox" [checked]="data.settings.respectMothballFiles" (change)="patch({ respectMothballFiles: !data.settings.respectMothballFiles })" /> Respect .mothball files in project roots</label>
        <div class="schedule">
          <span>Delete mode</span>
          <select [value]="data.settings.deleteMode" (change)="patch({ deleteMode: selectValue($event) })">
            <option value="permanent">Permanent (recommended)</option>
            <option value="trash">Recycle Bin / Trash</option>
          </select>
        </div>
      </section>
      @if (error()) { <div class="banner">{{ error() }}</div> }
    }
  `,
})
export class SettingsComponent implements OnInit {
  private readonly api = inject(ApiService);
  readonly view = signal<SettingsResponse | null>(null);
  readonly error = signal('');

  async ngOnInit() { await this.reload(); }
  async reload() { this.view.set(await this.api.settings()); }

  async patch(change: Partial<SettingsResponse['settings']>) {
    const current = this.view();
    if (!current) return;
    const settings = { ...current.settings, ...change };
    await this.api.saveSettings(settings);
    await this.reload();
  }

  async setDays(event: Event) {
    await this.patch({ thresholdDays: Number((event.target as HTMLInputElement).value) });
  }

  async addFolder() {
    const picked = await this.api.pickFolder();
    if (picked) {
      await this.api.addRoot(picked);
    } else if (!this.api.inTauri) {
      const typed = prompt('Folder to watch');
      if (!typed) return;
      await this.api.addRoot(typed);
    } else {
      return;
    }
    await this.reload();
  }

  async remove(path: string) {
    await this.api.removeRoot(path);
    await this.reload();
  }

  async toggleRule(rule: RuleCard) {
    const current = this.view();
    if (!current) return;
    const settings = structuredClone(current.settings);
    settings.ecosystems[rule.id] = { enabled: !rule.enabled, optIn: rule.enabledOptIn };
    await this.api.saveSettings(settings);
    await this.reload();
  }

  async toggleOpt(rule: RuleCard, name: string) {
    const current = this.view();
    if (!current) return;
    const settings = structuredClone(current.settings);
    const selected = new Set(rule.enabledOptIn);
    if (selected.has(name)) selected.delete(name); else selected.add(name);
    settings.ecosystems[rule.id] = { enabled: rule.enabled, optIn: [...selected] };
    await this.api.saveSettings(settings);
    await this.reload();
  }

  timeValue(event: Event) { return (event.target as HTMLInputElement).value; }
  numberValue(event: Event) { return Number((event.target as HTMLSelectElement).value); }
  selectValue(event: Event) { return (event.target as HTMLSelectElement).value as 'permanent' | 'trash'; }
}
