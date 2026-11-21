// VerseEngine : pont entre Azgaar (servi dans un bac à sable) et l'activité Discord qui l'affiche.
// Seul échange possible : la fenêtre parente demande la carte, Azgaar répond avec le contenu du
// `.map` (le même texte que « Enregistrer »). Rien d'autre n'est lu ni envoyé.

const REQUEST = "verseengine:export-request";

const embedded = window.parent !== window;

window.addEventListener("message", event => {
  if (!embedded || event.source !== window.parent) return;
  const message = event.data as { type?: unknown; id?: unknown } | null;
  if (!message || message.type !== REQUEST || typeof message.id !== "number") return;
  const id = message.id;
  // Réponse à la seule fenêtre qui a demandé, à son origine.
  const reply = (body: Record<string, unknown>) => window.parent.postMessage({ ...body, id }, event.origin || "*");
  import("@/services")
    // `Services.Save` est chargé à la demande : ses fonctions renvoient une promesse.
    .then(async ({ Services }) => {
      const data: unknown = await Services.Save.prepareMapData();
      if (typeof data !== "string") throw new Error("map data is not text");
      reply({ type: "verseengine:map", data });
    })
    .catch(error => reply({ type: "verseengine:error", message: String(error) }));
});

// Prévient l'activité qu'une carte est prête (génération ou chargement terminé).
window.addEventListener("map:generated", () => {
  if (embedded) window.parent.postMessage({ type: "verseengine:ready" }, "*");
});
