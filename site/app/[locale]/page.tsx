import { notFound } from "next/navigation";
import { isLocale, type Locale } from "@/i18n/config";
import { getDictionary } from "@/i18n/dictionaries";
import { Header } from "@/components/Header";
import { Footer } from "@/components/Footer";
import { Hero } from "@/components/sections/Hero";
import { Trust } from "@/components/sections/Trust";
import { Features } from "@/components/sections/Features";
import { HowItWorks } from "@/components/sections/HowItWorks";
import { Screenshots } from "@/components/sections/Screenshots";
import { Roadmap } from "@/components/sections/Roadmap";
import { Download } from "@/components/sections/Download";
import { Faq } from "@/components/sections/Faq";

export default async function Page({
  params,
}: {
  params: Promise<{ locale: string }>;
}) {
  const { locale } = await params;
  if (!isLocale(locale)) notFound();
  const l = locale as Locale;
  const dict = await getDictionary(l);

  return (
    <>
      <Header locale={l} nav={dict.nav} />
      <main id="main">
        <Hero dict={dict} />
        <Trust dict={dict} />
        <Features dict={dict} />
        <HowItWorks dict={dict} />
        <Screenshots dict={dict} />
        <Roadmap dict={dict} />
        <Download dict={dict} />
        <Faq dict={dict} />
      </main>
      <Footer locale={l} dict={dict} />
    </>
  );
}
