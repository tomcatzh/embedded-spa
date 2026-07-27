import {
  constants as zlibConstants,
  brotliCompressSync,
  gzipSync,
} from "node:zlib";
import { readdir, readFile, stat, writeFile } from "node:fs/promises";
import { extname, join, relative, resolve } from "node:path";

const requestedDirectory = process.argv[2];

if (!requestedDirectory) {
  console.error("usage: node scripts/precompress.mjs <static-output-directory>");
  process.exit(2);
}

const outputDirectory = resolve(requestedDirectory);
const outputStat = await stat(outputDirectory).catch(() => null);

if (!outputStat?.isDirectory()) {
  console.error(`static output directory does not exist: ${outputDirectory}`);
  process.exit(2);
}

const compressibleExtensions = new Set([
  ".css",
  ".html",
  ".js",
  ".json",
  ".svg",
  ".txt",
  ".webmanifest",
  ".xml",
]);

async function walk(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];

  for (const entry of entries) {
    const path = join(directory, entry.name);

    if (entry.isDirectory()) files.push(...(await walk(path)));
    else files.push(path);
  }

  return files;
}

let compressed = 0;

for (const path of await walk(outputDirectory)) {
  if (!compressibleExtensions.has(extname(path))) continue;

  const input = await readFile(path);
  const brotli = brotliCompressSync(input, {
    params: {
      [zlibConstants.BROTLI_PARAM_QUALITY]: 11,
    },
  });
  const gzip = gzipSync(input, { level: 9 });

  await Promise.all([
    writeFile(`${path}.br`, brotli),
    writeFile(`${path}.gz`, gzip),
  ]);

  compressed += 1;
  console.log(
    `precompressed ${relative(outputDirectory, path)} ` +
      `(${input.byteLength} -> br ${brotli.byteLength}, gzip ${gzip.byteLength})`,
  );
}

console.log(`precompressed ${compressed} static asset(s)`);
