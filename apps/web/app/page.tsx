"use client";

import { useI18n } from "@/lib/i18n";

import { HomeFooter } from "./home/footer";
import { Cta } from "./home/cta";
import { Features } from "./home/features";
import { Hero } from "./home/hero";
import { HomeNav } from "./home/nav";
import { Pricing } from "./home/pricing";
import { Assurance } from "./home/assurance";
import { Steps } from "./home/steps";

export default function HomePage() {
  const { t } = useI18n();

  return (
    <>
      <a href="#main" className="skip-link">
        {t.common.skipToContent}
      </a>

      <HomeNav />

      <main
        id="main"
        tabIndex={-1}
        className="mx-auto w-full max-w-[1080px] px-4 sm:px-8"
      >
        <Hero />

        <div className="flex flex-col gap-16 sm:gap-20">
          <Assurance />
          <Features />
          <Steps />
          <Pricing />
          <Cta />
        </div>
      </main>

      <HomeFooter />
    </>
  );
}
