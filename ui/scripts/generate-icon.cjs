// Generate a simple icon using canvas
const fs = require('fs');
const path = require('path');

// Create a simple 256x256 PNG using raw pixel data
// This is a minimal PNG with cyan color

// PNG signature
const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);

function crc32(data) {
    let crc = 0xffffffff;
    const table = new Uint32Array(256);
    for (let i = 0; i < 256; i++) {
        let c = i;
        for (let j = 0; j < 8; j++) {
            c = (c & 1) ? (0xedb88320 ^ (c >>> 1)) : (c >>> 1);
        }
        table[i] = c;
    }
    for (let i = 0; i < data.length; i++) {
        crc = table[(crc ^ data[i]) & 0xff] ^ (crc >>> 8);
    }
    return (crc ^ 0xffffffff) >>> 0;
}

function createChunk(type, data) {
    const length = Buffer.alloc(4);
    length.writeUInt32BE(data.length);
    
    const typeData = Buffer.from(type);
    const crcData = Buffer.concat([typeData, data]);
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(crc32(crcData));
    
    return Buffer.concat([length, typeData, data, crc]);
}

// IHDR chunk (image header)
const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(32, 0);  // width
ihdr.writeUInt32BE(32, 4);  // height
ihdr[8] = 8;  // bit depth
ihdr[9] = 2;  // color type (RGB)
ihdr[10] = 0; // compression
ihdr[11] = 0; // filter
ihdr[12] = 0; // interlace

// Create raw image data (32x32 RGB)
const rawData = Buffer.alloc(32 * (1 + 32 * 3)); // filter byte + RGB per row
for (let y = 0; y < 32; y++) {
    rawData[y * (1 + 32 * 3)] = 0; // filter none
    for (let x = 0; x < 32; x++) {
        const idx = y * (1 + 32 * 3) + 1 + x * 3;
        // Cyan color
        rawData[idx] = 0x06;     // R
        rawData[idx + 1] = 0xB6; // G
        rawData[idx + 2] = 0xD4; // B
    }
}

// Compress with zlib
const zlib = require('zlib');
const compressed = zlib.deflateSync(rawData);

// IEND chunk
const iend = Buffer.alloc(0);

// Build PNG
const png = Buffer.concat([
    signature,
    createChunk('IHDR', ihdr),
    createChunk('IDAT', compressed),
    createChunk('IEND', iend)
]);

// Save PNG
const pngPath = path.join(__dirname, '..', '..', 'src-tauri', 'icons', 'icon.png');
fs.mkdirSync(path.dirname(pngPath), { recursive: true });
fs.writeFileSync(pngPath, png);
console.log('Created icon.png');

// Create ICO from PNG
const icoPath = path.join(__dirname, '..', '..', 'src-tauri', 'icons', 'icon.ico');

// Read the PNG we created
const pngData = fs.readFileSync(pngPath);

// ICO header
const header = Buffer.alloc(6);
header.writeUInt16LE(0, 0);     // Reserved
header.writeUInt16LE(1, 2);     // Type (1 = ICO)
header.writeUInt16LE(1, 4);     // Number of images

// ICO directory entry
const entry = Buffer.alloc(16);
entry.writeUInt8(32, 0);        // Width
entry.writeUInt8(32, 1);        // Height
entry.writeUInt8(0, 2);         // Colors (0 = >256)
entry.writeUInt8(0, 3);         // Reserved
entry.writeUInt16LE(1, 4);      // Color planes
entry.writeUInt16LE(32, 6);     // Bits per pixel
entry.writeUInt32LE(pngData.length, 8);   // Size of image data
entry.writeUInt32LE(22, 12);    // Offset to image data

const ico = Buffer.concat([header, entry, pngData]);
fs.writeFileSync(icoPath, ico);
console.log('Created icon.ico');
