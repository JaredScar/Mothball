import { Component, inject } from '@angular/core';
import { RouterOutlet } from '@angular/router';
import { ApiService } from './core/api.service';
import { TrayComponent } from './tray.component';
import { ToastComponent } from './toast.component';

@Component({
  selector: 'app-root',
  imports: [RouterOutlet, TrayComponent, ToastComponent],
  template: `
    @switch (api.windowKind) {
      @case ('tray') { <app-tray /> }
      @case ('toast') { <app-toast /> }
      @default { <router-outlet /> }
    }
  `,
  styleUrl: './app.component.css',
})
export class AppComponent {
  readonly api = inject(ApiService);
}
