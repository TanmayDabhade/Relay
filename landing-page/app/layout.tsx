import type { Metadata } from "next";
import { Unbounded, JetBrains_Mono } from "next/font/google";
import { Analytics } from "@vercel/analytics/next"
import "./globals.css";

const unbounded = Unbounded({
  variable: "--font-unbounded",
  subsets: ["latin"],
  weight: ["400", "700", "900"],
});

const jetbrainsMono = JetBrains_Mono({
  variable: "--font-jetbrains-mono",
  subsets: ["latin"],
  weight: ["400", "500", "700"],
});

export const metadata: Metadata = {
  title: "Relay | Run your agents locally",
  description:
    "Relay is the native-first project management & observability layer for AI coding agents on your machine. Track spend, replay sessions, and deploy tasks straight from a Kanban that spawns real terminals.",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
        <html lang="en" className={`${unbounded.variable} ${jetbrainsMono.variable}`}>
      <body className="min-h-screen bg-relay-bg font-mono text-relay-text antialiased">
        {children}
        <Analytics />
      </body>
    </html>
    
  );
}
