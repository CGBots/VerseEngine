import * as THREE from 'three';
import { initials } from './player';

/** Étiquette de lieu (texte sur fond sombre) dessinée dans un canvas. */
export function labelTexture(text: string): { texture: THREE.CanvasTexture; aspect: number } {
  const font = '600 44px system-ui, -apple-system, "Segoe UI", Roboto, sans-serif';
  const c = document.createElement('canvas');
  const ctx = c.getContext('2d')!;
  ctx.font = font;
  const w = Math.ceil(ctx.measureText(text).width) + 36;
  c.width = w;
  c.height = 64;
  ctx.font = font;
  ctx.fillStyle = 'rgba(20, 21, 24, 0.78)';
  ctx.beginPath();
  ctx.roundRect(0, 4, w, 56, 28);
  ctx.fill();
  ctx.fillStyle = '#f2f3f5';
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  ctx.fillText(text, w / 2, 34);
  const texture = new THREE.CanvasTexture(c);
  texture.colorSpace = THREE.SRGBColorSpace;
  return { texture, aspect: w / 64 };
}

/**
 * Épingle « goutte inversée » avec la photo du joueur dans la partie ronde. La pointe est en bas au
 * centre du canvas (le sprite est ancré sur elle). Sans photo, ou si elle ne charge pas, les
 * initiales du personnage sont affichées.
 */
export function pinTexture(pictureUrl: string | null, name: string, onReady: () => void): THREE.CanvasTexture {
  const W = 128, H = 176, R = 56, CY = 62;
  const c = document.createElement('canvas');
  c.width = W;
  c.height = H;
  const ctx = c.getContext('2d')!;
  const texture = new THREE.CanvasTexture(c);
  texture.colorSpace = THREE.SRGBColorSpace;

  const drawShape = () => {
    ctx.clearRect(0, 0, W, H);
    ctx.save();
    ctx.shadowColor = 'rgba(0,0,0,0.45)';
    ctx.shadowBlur = 8;
    ctx.shadowOffsetY = 3;
    ctx.fillStyle = '#ed4245';
    ctx.beginPath();
    const a = Math.asin(R / (H - 6 - CY));
    ctx.moveTo(W / 2, H - 6);
    ctx.lineTo(W / 2 - R * Math.cos(a), CY + R * Math.sin(a));
    ctx.arc(W / 2, CY, R, Math.PI - a, a, false);
    ctx.closePath();
    ctx.fill();
    ctx.restore();
  };
  const drawInner = (img: HTMLImageElement | null) => {
    drawShape();
    ctx.save();
    ctx.beginPath();
    ctx.arc(W / 2, CY, R - 7, 0, Math.PI * 2);
    ctx.clip();
    if (img) {
      ctx.drawImage(img, W / 2 - (R - 7), CY - (R - 7), (R - 7) * 2, (R - 7) * 2);
    } else {
      ctx.fillStyle = '#5865f2';
      ctx.fillRect(0, 0, W, H);
      ctx.fillStyle = '#fff';
      ctx.font = '700 44px system-ui, sans-serif';
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.fillText(initials(name), W / 2, CY + 2);
    }
    ctx.restore();
    texture.needsUpdate = true;
    onReady();
  };

  drawInner(null);
  if (pictureUrl) {
    const img = new Image();
    // Le CDN Discord répond avec Access-Control-Allow-Origin: * : le canvas reste utilisable par WebGL.
    img.crossOrigin = 'anonymous';
    img.onload = () => drawInner(img);
    img.src = pictureUrl;
  }
  return texture;
}

/** Proportions de l'épingle (largeur / hauteur). */
export const PIN_ASPECT = 128 / 176;
