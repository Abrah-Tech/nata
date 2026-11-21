import type { Metadata } from "next";

import { I18nProvider } from "@/lib/i18n";

import { RouteAnnouncer } from "./route-announcer";
import "./globals.css";

export const metadata: Metadata = {
  title: "Nata",
  description: "Production planning and cost control for small F&B businesses",
};

export default function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="id">
      <body>
        {/* Marks that JavaScript is running, before the body renders. The
            landing page scopes its reveal animation to `html.js` so a band is
            never visible one frame and hidden the next. */}
        <script
          dangerouslySetInnerHTML={{
            __html: "document.documentElement.classList.add('js')",
          }}
        />
        <I18nProvider>
          <RouteAnnouncer />
          {children}
        </I18nProvider>
      </body>
    </html>
  );
}
