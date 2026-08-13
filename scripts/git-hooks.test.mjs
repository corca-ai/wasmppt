import assert from 'node:assert/strict'
import { chmod, cp, mkdtemp, mkdir, readFile, rm, stat, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { spawnSync } from 'node:child_process'
import test from 'node:test'

function run(command, args, options = {}) {
  return spawnSync(command, args, { encoding: 'utf8', ...options })
}

test('tracked hooks install with the expected path, mode, and command mapping', async (context) => {
  const temporaryRoot = await mkdtemp(join(tmpdir(), 'wasmppt-hooks-'))
  context.after(() => rm(temporaryRoot, { recursive: true, force: true }))
  await mkdir(join(temporaryRoot, '.githooks'))
  await mkdir(join(temporaryRoot, 'scripts'))
  await cp(new URL('../.githooks/pre-commit', import.meta.url), join(temporaryRoot, '.githooks/pre-commit'))
  await cp(new URL('../.githooks/pre-push', import.meta.url), join(temporaryRoot, '.githooks/pre-push'))
  await cp(
    new URL('install-git-hooks.sh', import.meta.url),
    join(temporaryRoot, 'scripts/install-git-hooks.sh'),
  )
  await chmod(join(temporaryRoot, '.githooks/pre-commit'), 0o755)
  await chmod(join(temporaryRoot, '.githooks/pre-push'), 0o755)
  await chmod(join(temporaryRoot, 'scripts/install-git-hooks.sh'), 0o755)

  assert.equal(run('git', ['init', '--quiet'], { cwd: temporaryRoot }).status, 0)
  const install = run('sh', ['scripts/install-git-hooks.sh'], { cwd: temporaryRoot })
  assert.equal(install.status, 0, install.stderr)
  const hooksPath = run('git', ['config', '--local', '--get', 'core.hooksPath'], {
    cwd: temporaryRoot,
  })
  assert.equal(hooksPath.stdout.trim(), '.githooks')
  assert.notEqual((await stat(join(temporaryRoot, '.githooks/pre-commit'))).mode & 0o111, 0)
  assert.notEqual((await stat(join(temporaryRoot, '.githooks/pre-push'))).mode & 0o111, 0)

  const hook = await readFile(join(temporaryRoot, '.githooks/pre-commit'), 'utf8')
  assert.match(hook, /npm run precommit/)
  assert.match(hook, /reproduce with: npm run precommit/)
  const prePush = await readFile(join(temporaryRoot, '.githooks/pre-push'), 'utf8')
  assert.match(prePush, /cat > "\$push_refs_file"/)
  assert.match(prePush, /npm run prepush/)
  assert.match(prePush, /reproduce with: npm run prepush/)
})

test('installer rejects a tracked hook without execute permission', async (context) => {
  const temporaryRoot = await mkdtemp(join(tmpdir(), 'wasmppt-hooks-mode-'))
  context.after(() => rm(temporaryRoot, { recursive: true, force: true }))
  await mkdir(join(temporaryRoot, '.githooks'))
  await mkdir(join(temporaryRoot, 'scripts'))
  await writeFile(join(temporaryRoot, '.githooks/pre-commit'), '#!/bin/sh\n')
  await cp(
    new URL('install-git-hooks.sh', import.meta.url),
    join(temporaryRoot, 'scripts/install-git-hooks.sh'),
  )
  assert.equal(run('git', ['init', '--quiet'], { cwd: temporaryRoot }).status, 0)

  const install = run('sh', ['scripts/install-git-hooks.sh'], { cwd: temporaryRoot })
  assert.equal(install.status, 1)
  assert.match(install.stderr, /not executable: \.githooks\/pre-commit/)
})

test('pre-push preserves ref input and propagates gate failure', async (context) => {
  const temporaryRoot = await mkdtemp(join(tmpdir(), 'wasmppt-pre-push-'))
  context.after(() => rm(temporaryRoot, { recursive: true, force: true }))
  await mkdir(join(temporaryRoot, '.githooks'))
  await mkdir(join(temporaryRoot, 'bin'))
  await cp(new URL('../.githooks/pre-push', import.meta.url), join(temporaryRoot, '.githooks/pre-push'))
  await chmod(join(temporaryRoot, '.githooks/pre-push'), 0o755)
  await writeFile(
    join(temporaryRoot, 'bin/npm'),
    '#!/bin/sh\n[ "$1 $2" = "run prepush" ] || exit 97\ngrep "refs/heads/main" "$WASMPPT_PRE_PUSH_REFS" >/dev/null || exit 98\nexit 23\n',
  )
  await chmod(join(temporaryRoot, 'bin/npm'), 0o755)
  assert.equal(run('git', ['init', '--quiet'], { cwd: temporaryRoot }).status, 0)

  const refs = 'refs/heads/main 0123 refs/heads/main 4567\n'
  const result = run('sh', ['.githooks/pre-push'], {
    cwd: temporaryRoot,
    env: { ...process.env, PATH: `${join(temporaryRoot, 'bin')}:${process.env.PATH}` },
    input: refs,
  })
  assert.equal(result.status, 1)
  assert.match(result.stderr, /reproduce with: npm run prepush/)
})
