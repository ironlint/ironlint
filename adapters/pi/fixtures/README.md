# Pi fixture record

The checked-in JSON files are historical, provenance-stamped `tool_call`
payloads captured from Pi 0.84.3. The current adapter listens for Pi 0.85.1
`tool_result` events after supported writes complete, so it does not consume
these files. They document the retired protocol and are not evidence for the
current feedback path.

The active Node tests cover the current event shape with synthetic inputs. See
each JSON file's `_provenance` header for the historical capture details.
