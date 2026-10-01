# Pi fixture record

The checked-in JSON files are historical, provenance-stamped `tool_call`
payloads captured from Pi 0.84.3. The current adapter listens for Pi 0.85.1
`tool_result` events after supported writes complete, so it does not consume
these files. They document the retired protocol and are not evidence for the
current feedback path.

The active Node tests cover the current event shape with synthetic inputs. See
each JSON file's `_provenance` header for the historical capture details.

The controlled completion tests in `../test/compliance/` are separate from
these historical captures. They run the pinned Pi 0.87.1 SDK with a scripted
provider; `complete-real.test.ts` also invokes the real IronLint CLI. The
qualification record in `../completion.md` states exactly what those runs
prove and which live model measurements remain pending.
