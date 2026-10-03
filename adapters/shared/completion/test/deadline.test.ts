import { test } from "node:test"
import assert from "node:assert/strict"
import { Deadline } from "../deadline.ts"

test("one monotonic deadline reserves cleanup and counts every phase", () => {
  let now = 10
  const deadline = new Deadline(9_000, undefined, () => now)
  try {
    assert.equal(deadline.remainingMs(), 7_000)
    now += 3_000
    assert.equal(deadline.remainingMs(), 4_000)
    now += 4_000
    assert.throws(() => deadline.check(), /deadline/i)
    assert.equal(deadline.signal.aborted, true)
    assert.equal(deadline.remainingMs(true), 2_000)
    now += 2_000
    assert.equal(deadline.remainingMs(true), 0)
  } finally { deadline.dispose() }
})

test("caller cancellation is terminal and a fresh attempt has no inherited state", () => {
  const controller = new AbortController()
  const first = new Deadline(10_000, controller.signal)
  controller.abort()
  assert.throws(() => first.check(), /cancel/i)
  first.dispose()
  const fresh = new Deadline(10_000)
  assert.doesNotThrow(() => fresh.check())
  fresh.dispose()
})
