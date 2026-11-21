import { inject } from '@angular/core';
import { CanActivateFn, Router, Routes } from '@angular/router';
import { ApiService } from './core/api.service';
import { ShellComponent } from './shell.component';
import { ProjectsComponent } from './pages/projects.component';
import { PreviewComponent } from './pages/preview.component';
import { HistoryComponent } from './pages/history.component';
import { CachesComponent } from './pages/caches.component';
import { SettingsComponent } from './pages/settings.component';
import { OnboardingComponent } from './pages/onboarding.component';

const ready: CanActivateFn = async () => {
  const api = inject(ApiService);
  const router = inject(Router);
  const dash = await api.dashboard();
  return dash.onboarded ? true : router.createUrlTree(['/onboarding']);
};

export const routes: Routes = [
  { path: 'onboarding', component: OnboardingComponent },
  {
    path: '',
    component: ShellComponent,
    canActivate: [ready],
    children: [
      { path: '', pathMatch: 'full', redirectTo: 'projects' },
      { path: 'projects', component: ProjectsComponent },
      { path: 'preview', component: PreviewComponent },
      { path: 'history', component: HistoryComponent },
      { path: 'caches', component: CachesComponent },
      { path: 'settings', component: SettingsComponent },
    ],
  },
];
