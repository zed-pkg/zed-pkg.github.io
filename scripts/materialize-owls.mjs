import { createHash } from 'node:crypto';
import { mkdir, rm, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const repositoryRoot = fileURLToPath(new URL('../', import.meta.url));
const outputRoot = join(repositoryRoot, 'public', 'owls', 'vendor');
const sources = [
  {
    owner: 'ores-wasm-loaders',
    repo: 'owls-interfaces',
    commit: '231510f5d01046af657be42a5d4215be12622042',
    files: [
      { path: 'index.mjs', blob: '392947dc038f7b2276bde841c92e9f177a825f44' },
      { path: 'release.mjs', blob: 'a0bbc48e2350cdb9ba70f47af6102203f258c7b0' },
      { path: 'validate.mjs', blob: '4139700eed77c9e047d7c22261a91c4a2e528be1' },
      { path: 'schemas/release.schema.json', blob: '56dec8a37d7fdd56056fd5803f3496da8f05c233' },
    ],
  },
  {
    owner: 'ores-wasm-loaders',
    repo: 'owls-web-loader',
    commit: 'deae23537d27aed94bdc2510649f99393379a617',
    files: [
      { path: 'src/coordinator.mjs', blob: 'ce49605849c2cd38451dcc6ca5a66bef97ee7d97' },
      { path: 'src/adapters.mjs', blob: 'db7a70ac91acd6463fdfa9d5366b1cada8c05aea' },
      { path: 'src/hints.mjs', blob: 'd3a1acc964c3c8286bd3bb53967a9dcec5c7de27' },
      { path: 'src/contract.mjs', blob: '6a0abb73bbb3db287567a66ec00a83f020b7f194' },
      { path: 'src/transport.mjs', blob: '36ad0dfca95d9c6a9e89388ab62deb509b0831d5' },
      { path: 'src/ownership.mjs', blob: '01691e6e9a18ca11184aa8acccc63da59867538b' },
      { path: 'src/manifest-fetch.mjs', blob: '362078381391e1e2f0907a753e227d158c12a2c9' },
      {
        path: 'fixtures/empty.wasm',
        blob: 'd8fc92d022fbf4d1072da17bc8e0840054b51ddc',
        sha256: '93a44bbb96c751218e4c00d479e4c14358122a389acca16205b1e4d0dc5f9476'
      }
    ]
  }
];

function gitBlobSha(bytes) {
  const header = Buffer.from(`blob ${bytes.byteLength}\0`);
  return createHash('sha1').update(header).update(bytes).digest('hex');
}

async function fetchBytes(url) {
  let lastError;
  for (let attempt = 1; attempt <= 3; attempt += 1) {
    try {
      const response = await fetch(url, {
        redirect: 'error',
        headers: {
          accept: 'application/octet-stream',
          'user-agent': 'zed-pkg-site-owls-materializer/1'
        }
      });
      if (!response.ok) {
        const error = new Error(`HTTP ${response.status} while fetching ${url}`);
        if (response.status < 500 && response.status !== 429) throw error;
        lastError = error;
      } else {
        return Buffer.from(await response.arrayBuffer());
      }
    } catch (error) {
      lastError = error;
    }
    if (attempt < 3) await new Promise((resolve) => setTimeout(resolve, attempt * 250));
  }
  throw lastError ?? new Error(`Unable to fetch ${url}`);
}

async function materialize(source, file) {
  const url = `https://raw.githubusercontent.com/${source.owner}/${source.repo}/${source.commit}/${file.path}`;
  const bytes = await fetchBytes(url);
  const actualBlob = gitBlobSha(bytes);
  if (actualBlob !== file.blob) {
    throw new Error(`${source.repo}/${file.path} Git blob mismatch: expected ${file.blob}, got ${actualBlob}`);
  }
  const actualSha256 = createHash('sha256').update(bytes).digest('hex');
  if (file.sha256 && actualSha256 !== file.sha256) {
    throw new Error(`${source.repo}/${file.path} SHA-256 mismatch: expected ${file.sha256}, got ${actualSha256}`);
  }

  const destination = join(outputRoot, source.repo, ...file.path.split('/'));
  await mkdir(dirname(destination), { recursive: true });
  await writeFile(destination, bytes);
  return {
    source: `${source.owner}/${source.repo}@${source.commit}`,
    path: file.path,
    blob: actualBlob,
    sha256: actualSha256,
    bytes: bytes.byteLength
  };
}

await rm(outputRoot, { recursive: true, force: true });
const receipts = [];
for (const source of sources) {
  for (const file of source.files) receipts.push(await materialize(source, file));
}
await writeFile(
  join(outputRoot, 'manifest.json'),
  `${JSON.stringify({
    schemaVersion: 1,
    sources: sources.map(({ owner, repo, commit }) => ({ owner, repo, commit })),
    files: receipts
  }, null, 2)}\n`
);
console.log(`Materialized ${receipts.length} verified OWLS files under public/owls/vendor`);
