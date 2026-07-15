import { LoginForm } from "../components/LoginForm";

export default function LoginPage() {
  return (
    <main className="mx-auto flex min-h-screen max-w-sm flex-col justify-center px-6 py-24 text-center">
      <h1 className="font-display text-2xl font-black tracking-tight uppercase">
        Sign in
      </h1>
      <LoginForm next="/waitlist" />
    </main>
  );
}
