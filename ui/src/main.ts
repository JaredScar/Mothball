import { bootstrapApplication } from '@angular/platform-browser';
import { provideZoneChangeDetection } from '@angular/core';
import { appConfig } from './app/app.config';
import { AppComponent } from './app/app.component';

const win = window as unknown as { __MOTHBALL_WINDOW__?: string };
const requested = win.__MOTHBALL_WINDOW__ || new URLSearchParams(location.search).get('window') || 'main';
const config = requested === 'tray' || requested === 'toast'
  ? { providers: [provideZoneChangeDetection({ eventCoalescing: true })] }
  : appConfig;

bootstrapApplication(AppComponent, config)
  .catch((err) => console.error(err));
