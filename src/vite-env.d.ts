/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_SUPABASE_URL: string;
  readonly VITE_SUPABASE_ANON_KEY: string;
  /** Landing site's billing/upgrade page — checkout isn't built yet (see the auth design
   * notes), so this is optional; the profile page disables "Upgrade" until it's set. */
  readonly VITE_LANDING_URL?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
