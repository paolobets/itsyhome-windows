#!/usr/bin/env node
'use strict';
/**
 * Generates tray icons for ItsyHome:
 *   resources/tray-default.png   (blue house – connected)
 *   resources/tray-error.png     (red  house – error)
 *   resources/tray-connecting.png (orange house – connecting)
 *
 * Uses only Node.js built-ins (zlib), no npm deps needed.
 * Run: node scripts/gen-icon.cjs
 */

const { deflateSync } = require('zlib');
const fs   = require('fs');
const path = require('path');

// ─── Minimal PNG encoder ──────────────────────────────────────────────────────

const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let i = 0; i < 256; i++) {
    let c = i;
    for (let j = 0; j < 8; j++) c = (c & 1) ? 0xEDB88320 ^ (c >>> 1) : c >>> 1;
    t[i] = c >>> 0;
  }
  return t;
})();

function crc32(buf) {
  let crc = 0xFFFFFFFF;
  for (let i = 0; i < buf.length; i++)
    crc = CRC_TABLE[(crc ^ buf[i]) & 0xFF] ^ (crc >>> 8);
  return (crc ^ 0xFFFFFFFF) >>> 0;
}

function chunk(type, data) {
  const t = Buffer.from(type, 'ascii');
  const d = Buffer.isBuffer(data) ? data : Buffer.from(data);
  const len = Buffer.alloc(4); len.writeUInt32BE(d.length);
  const crcVal = Buffer.alloc(4);
  crcVal.writeUInt32BE(crc32(Buffer.concat([t, d])));
  return Buffer.concat([len, t, d, crcVal]);
}

function makePNG(w, h, pixels /* Uint8Array RGBA flat */) {
  const rows = [];
  for (let y = 0; y < h; y++) {
    rows.push(0); // filter type: None
    for (let x = 0; x < w; x++) {
      const i = (y * w + x) * 4;
      rows.push(pixels[i], pixels[i+1], pixels[i+2], pixels[i+3]);
    }
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8; ihdr[9] = 6; // 8-bit RGBA
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), // PNG sig
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(Buffer.from(rows))),
    chunk('IEND', Buffer.alloc(0))
  ]);
}

// ─── Draw house icon ──────────────────────────────────────────────────────────

function drawHouse(W, H, r, g, b) {
  const px = new Uint8Array(W * H * 4); // all transparent

  function set(x, y, alpha = 255) {
    if (x < 0 || x >= W || y < 0 || y >= H) return;
    const i = (y * W + x) * 4;
    px[i] = r; px[i+1] = g; px[i+2] = b; px[i+3] = alpha;
  }

  function fillRow(y, xL, xR, alpha = 255) {
    for (let x = xL; x <= xR; x++) set(x, y, alpha);
  }

  function fillRect(x1, y1, x2, y2, alpha = 255) {
    for (let y = y1; y <= y2; y++) fillRow(y, x1, x2, alpha);
  }

  if (W === 16) {
    // Roof: rows 1-7, triangular, centered on x=7.5
    //   row 1: x=7-8 (2px peak)
    //   row 7: x=1-14 (14px base)
    for (let row = 1; row <= 7; row++) {
      const half = row;               // grows 1 per row
      fillRow(row, 8 - half, 7 + half);
    }
    // Walls: rows 8-13, x=2..13
    fillRect(2, 8, 13, 13);
    // Door: rows 10-13, x=6..9 – transparent cutout
    fillRect(6, 10, 9, 13, 0); // alpha=0

  } else if (W === 32) {
    // Scale everything x2
    // Roof: rows 2-14, triangular, centered on x=15.5
    for (let row = 2; row <= 14; row++) {
      const half = row;
      fillRow(row, 16 - half, 15 + half);
    }
    // Walls: rows 15-26, x=4..27
    fillRect(4, 15, 27, 26);
    // Door: rows 19-26, x=12..19
    fillRect(12, 19, 19, 26, 0);
  }

  return px;
}

// ─── Generate & save ──────────────────────────────────────────────────────────

const outDir = path.join(__dirname, '..', 'resources');
fs.mkdirSync(outDir, { recursive: true });

const icons = [
  { name: 'tray-default',    r: 0x03, g: 0xA9, b: 0xF4 }, // HA blue   #03A9F4
  { name: 'tray-error',      r: 0xFF, g: 0x45, b: 0x3A }, // iOS red   #FF453A
  { name: 'tray-connecting', r: 0xFF, g: 0x9F, b: 0x0A }, // iOS amber #FF9F0A
];

for (const { name, r, g, b } of icons) {
  const px16 = drawHouse(16, 16, r, g, b);
  const png  = makePNG(16, 16, px16);
  const file = path.join(outDir, `${name}.png`);
  fs.writeFileSync(file, png);
  console.log(`✓ ${file}  (${png.length} bytes)`);
}

console.log('\nDone! Icons saved to resources/');
