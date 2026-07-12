import { Nav } from "./components/Nav";
import { Hero } from "./components/Hero";
import { Features } from "./components/Features";
import { Manifesto } from "./components/Manifesto";
import { Compare } from "./components/Compare";
import { Pricing } from "./components/Pricing";
import { WaitlistCta } from "./components/WaitlistCta";
import { Footer } from "./components/Footer";

export default function Home() {
  return (
    <>
      <Nav />
      <main>
        <Hero />
        <Features />
        <Manifesto />
        <Compare />
        <Pricing />
        <WaitlistCta />
      </main>
      <Footer />
    </>
  );
}
