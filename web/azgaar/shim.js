// VerseEngine : chargé en premier dans la page d'Azgaar.
// Azgaar est servi dans un bac à sable (en-tête `Content-Security-Policy: sandbox`, origine opaque) :
// le navigateur lui refuse localStorage, sessionStorage et IndexedDB. Pour qu'il démarre quand même,
// on lui donne un stockage en mémoire, perdu à la fermeture (rien n'est écrit sur l'appareil).
(function () {
  function memoryStorage() {
    var items = new Map();
    return {
      get length() { return items.size; },
      key: function (i) { var k = Array.from(items.keys())[i]; return k === undefined ? null : k; },
      getItem: function (k) { k = String(k); return items.has(k) ? items.get(k) : null; },
      setItem: function (k, v) { items.set(String(k), String(v)); },
      removeItem: function (k) { items.delete(String(k)); },
      clear: function () { items.clear(); },
    };
  }
  var sandboxed = false;
  ['localStorage', 'sessionStorage'].forEach(function (name) {
    try {
      window[name].getItem('verseengine');
    } catch (e) {
      sandboxed = true;
      Object.defineProperty(window, name, { value: memoryStorage(), configurable: true });
    }
  });
  // Interface plus grande dans l'activité : Azgaar prend 1 sous 1280 px de large, trop petit dans
  // Discord. On lui donne une taille d'interface de départ (ancienne clé `uiSize`, reprise par
  // Azgaar au démarrage), bornée par ce que la fenêtre permet ; l'admin peut la changer dans les
  // options d'Azgaar. Seulement dans le bac à sable : jamais dans le stockage réel d'un navigateur.
  if (sandboxed) {
    var fit = Math.min(window.innerHeight / 465, window.innerWidth / 302);
    var size = Math.round(Math.min(1.5, Math.max(1.2, fit)) * 10) / 10;
    window.localStorage.setItem('uiSize', String(size));
  }
  try {
    // Hors bac à sable (développement), la base d'essai est aussitôt supprimée.
    var probe = window.indexedDB && window.indexedDB.open('verseengine-probe');
    if (probe) probe.onsuccess = function () { probe.result.close(); window.indexedDB.deleteDatabase('verseengine-probe'); };
  } catch (e) {
    Object.defineProperty(window, 'indexedDB', { value: undefined, configurable: true });
  }
})();
