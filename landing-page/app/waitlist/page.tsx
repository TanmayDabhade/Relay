import { redirect } from "next/navigation";
import { Nav } from "../components/Nav";
import { Hero } from "../components/Hero";
import { Footer } from "../components/Footer";
import { WaitlistStatus } from "../components/WaitlistStatus";
import { createSupabaseServerClient } from "@/lib/supabase-server";
import { getPositionForEmail } from "@/lib/waitlist";

// Reads the auth session, so this route must render per-request.
export const dynamic = "force-dynamic";

export default async function WaitlistPage() {
  const supabase = await createSupabaseServerClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user?.email) redirect("/");

  const position = await getPositionForEmail(user.email);
  if (position === null) redirect("/");

  return (
    <>
      <Nav />
      <main>
        <WaitlistStatus position={position} />
        <Hero />
      </main>
      <Footer />
    </>
  );
}
