# LLM payload specification for context generation

## 1. Design choice: full content, full response

- **Payload:** Current node content plus prompt (system and user). No previous context, no diff.
- **Response:** One full generation each time. No "previous context plus delta."
- **Rationale:** Simpler to implement, better for highly iterative files, avoids additive bloat. Inconsistent response structure is addressed by a response template.

## 2. Definition of "current node content"

- **File node:** The bytes of the file at `node_record.path`, decoded as UTF-8 for the prompt. If the file is not valid UTF-8, do not send raw bytes; see Binary files below.
- **Directory node:** For the generating agent and selected frame type, the current child context content: each child head frame for request `frame_type`, concatenated in deterministic order by path or node_id. No previous directory context in the payload.

So "current" is always: for files, the file on disk; for directories, that agent's current child frames' content.

## 3. Payload structure (what the LLM receives)

**System message:**  
Unchanged: agent's `system_prompt`. No content in the system message.

**User message for file nodes:**

- **Content block:** The current file content (UTF-8), with optional delimiters so the model can distinguish path from body, e.g. `<file path="{path}">\n{file_content}\n</file>`.
- **Task block:** The filled user prompt (placeholders `{path}`, `{node_type}`, `{file_size}` already substituted).
- **Optional response template:** If the agent defines a response template, append or inject instructions so the model's answer follows that structure (see Response template below).

**User message for directory nodes:**

- **Content block:** That agent child context content for request `frame_type`. Each child head frame for selected type is included, with clear separators per child path or id so model can identify each block.
- **Task block:** The filled `user_prompt_directory`.
- **Optional response template:** Same as for files.

In both cases the payload is: current content (file or that agent's child context) plus task (prompt) plus optional template.

## 4. Response template (consistent structure)

- **Purpose:** Constrain the LLM so responses have a stable structure (sections, format, length) across runs and agents.
- **Placement:** Agent metadata, e.g. `response_template` or `output_schema`. Value is a string injected into the user message (or system) that describes how to format the answer (e.g. "Respond with: Summary (2–3 sentences), Key points (bulleted list), Open questions (optional). Use markdown.").
- **Usage:** When building the user message, if `agent.metadata["response_template"]` is present, append or include it, e.g. `"\n\nRespond using this structure:\n" + response_template`, so the model sees the task plus the required structure.
- **Future:** Structured output (e.g. JSON schema) can be added later; for now a prose template is sufficient.

## 5. Binary and non-UTF-8 files

- **Spec:** If the file at `node_record.path` is not valid UTF-8, do not send raw bytes. Either send a placeholder in the content block (e.g. "Binary file (N bytes). No text content sent.") or omit the content block and only send path and size in the task.
- **Rationale:** Avoid corrupt or meaningless tokens and provider errors. A placeholder preserves "current content" semantics without sending binary.

## 6. Implementation location

- **Single place for content plus prompt to messages:** The frame generation queue in [src/frame/queue.rs](../../src/frame/queue.rs) builds LLM payload from queue request fields `node_id`, `path`, `node_type`, `agent_id`, `provider_name`, and `frame_type`. For file nodes queue reads file from `node_record.path`, UTF-8 or placeholder, and builds content block plus task block plus optional template. For directory nodes queue resolves child head frames for request agent and frame type, then builds content block plus task plus optional template.
- **No previous context in payload:** Do not pass existing frames for the same node as "previous context" for the model to diff against; those frames are for storage and history only. Diff-based regeneration is out of scope for this spec.

## 7. Summary

| Aspect | Choice |
|--------|--------|
| Content sent | Current file content (files) or current child context for that agent (directories). |
| Previous context | Not sent. Full content, full response (Option 1). |
| Structure consistency | Optional agent `response_template` in the user message. |
| Binary files | Placeholder or omit content; never send raw binary. |
| Per-agent retrieval | Already supported; directory content uses that agent's child heads only. |
