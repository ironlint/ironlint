import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { claudeArguments, classifyClaudeResult, loadClaudeConfig, parseClaudeConfig } from '../claude.ts';

test('Claude results require explicit successful terminal completion', () => {
  assert.equal(classifyClaudeResult(0, JSON.stringify({type:'result',subtype:'success',is_error:false,terminal_reason:'completed',result:'Done.'})), 'Done.');
  for (const result of [
    {type:'result',subtype:'success',is_error:false,terminal_reason:'hook_stopped',result:'Done.'},
    {type:'result',subtype:'success',is_error:false,result:'Done.'},
    {type:'result',subtype:'success',is_error:true,terminal_reason:'completed',result:'Done.'},
    {type:'result',subtype:'error_max_turns',is_error:true,terminal_reason:'max_turns'},
    {type:'result',subtype:'success',is_error:false,terminal_reason:'completed',result:'Done.',deferred_tool_use:{}},
  ]) assert.throws(() => classifyClaudeResult(0, JSON.stringify(result)));
  assert.throws(() => classifyClaudeResult(1, '{}'));
  assert.throws(() => classifyClaudeResult(0, '{'));
});

test('owner config accepts only bounded model provider settings', () => {
  assert.deepEqual(parseClaudeConfig({model:'claude-sonnet-4-6',env:{ANTHROPIC_API_KEY:'owner-key'}}), {model:'claude-sonnet-4-6',env:{ANTHROPIC_API_KEY:'owner-key'},maxTurns:8});
  for (const config of [{model:'x',maxTurns:0},{model:'x',maxTurns:17},{model:'x',env:{HOME:'/live'}},{model:'x',args:['--tools','Bash']},{model:''}]) assert.throws(() => parseClaudeConfig(config));
});

test('Claude controlled arguments exclude detached and arbitrary tools', () => {
  const args = claudeArguments(parseClaudeConfig({model:'claude-sonnet-4-6'}), '/tmp/candidate', '/tmp/owner-settings.json');
  assert.ok(args.includes('--bare'));
  assert.equal(args[args.indexOf('--tools')+1], 'Read,Edit');
  assert.ok(args[args.indexOf('--disallowedTools')+1].includes('Edit(//tmp/candidate/.git/**)'));
  assert.equal(args[args.indexOf('--permission-mode')+1], 'dontAsk');
  assert.ok(args.includes('--no-session-persistence'));
  assert.equal(args[args.indexOf('--setting-sources')+1], '');
  assert.ok(args.includes('--strict-mcp-config'));
  assert.equal(args[args.indexOf('--max-turns')+1], '8');
  assert.ok(!args.includes('--dangerously-skip-permissions'));
});

test('Claude config input rejects oversized and malformed UTF-8 files', async () => {
  const home = await mkdtemp(join(tmpdir(),'ironlint-claude-config-'));
  try {
    const path = join(home,'config.json');
    await writeFile(path, ' '.repeat(65537));
    await assert.rejects(loadClaudeConfig(path), /64 KiB/);
    await writeFile(path, Buffer.from([0x7b,0x22,0x6d,0x6f,0x64,0x65,0x6c,0x22,0x3a,0x22,0xff,0x22,0x7d]));
    await assert.rejects(loadClaudeConfig(path));
  } finally { await rm(home,{recursive:true,force:true}); }
});
