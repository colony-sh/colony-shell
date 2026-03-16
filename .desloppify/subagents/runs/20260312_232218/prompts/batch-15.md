You are a focused subagent reviewer for a single holistic investigation batch.

Repository root: /Users/mcfearsome/src/github.com/colony-sh/colony-shell
Blind packet: /Users/mcfearsome/src/github.com/colony-sh/colony-shell/.desloppify/review_packet_blind.json
Batch index: 15
Batch name: design_coherence
Batch rationale: seed files for design_coherence review

DIMENSION TO EVALUATE:

## design_coherence
Are structural design decisions sound — functions focused, abstractions earned, patterns consistent?
Look for:
- Functions doing too many things — multiple distinct responsibilities in one body
- Parameter lists that should be config/context objects — many related params passed together
- Files accumulating issues across many dimensions — likely mixing unrelated concerns
- Deep nesting that could be flattened with early returns or extraction
- Repeated structural patterns that should be data-driven
Skip:
- Functions that are long but have a single coherent responsibility
- Parameter lists where grouping would obscure meaning — do NOT recommend config/context objects or dependency injection wrappers just to reduce parameter count; only group when the grouping has independent semantic meaning
- Files that are large because their domain is genuinely complex, not because they mix concerns
- Nesting that is inherent to the problem (e.g., recursive tree processing)
- Do NOT recommend extracting callable parameters or injecting dependencies for 'testability' — direct function calls are simpler and preferred unless there is a concrete decoupling need

YOUR TASK: Read the code for this batch's dimension. Judge how well the codebase serves a developer from that perspective. The dimension rubric above defines what good looks like. Cite specific observations that explain your judgment.

Mechanical scan evidence — navigation aid, not scoring evidence:
The blind packet contains `holistic_context.scan_evidence` with aggregated signals from all mechanical detectors — including complexity hotspots, error hotspots, signal density index, boundary violations, and systemic patterns. Use these as starting points for where to look beyond the seed files.

Seed files (start here):
- crates/colony-exec/src/lib.rs
- crates/colony-expand/src/parameter.rs
- crates/colony-exec/src/builtins.rs
- crates/colony-exec/src/pipeline.rs
- crates/colony-shell/src/shell/execution/control_flow.rs
- crates/colony-shell/src/tutorial/tui/magic_pane.rs
- crates/colony-expand/src/arithmetic.rs
- crates/colony-shell/src/shell/execution/serialization.rs
- crates/colony-shell/src/repl_tui.rs
- crates/colony-shell/src/tutorial/explain/constructs.rs
- crates/colony-shell/src/repl.rs
- crates/colony-exec/src/flux/mod.rs
- crates/colony-env-bin/src/main.rs
- crates/colony-tui/src/prompt/renderer.rs
- crates/colony-channels/src/context.rs
- crates/colony-tui/src/input/command_palette.rs
- crates/colony-filetree-bin/src/main.rs
- crates/colony-history-bin/src/main.rs
- crates/colony-protocol/src/lib.rs
- crates/colony-tui/src/editor/buffer.rs
- crates/colony-agent/src/planning.rs
- crates/colony-agent/src/provider_chain.rs
- crates/colony-agent/src/providers/cloud.rs
- crates/colony-channels/src/protocol.rs
- crates/colony-cloud-sdk/src/timeout.rs
- crates/colony-config/src/trust.rs
- crates/colony-exec/src/buffers/buffer.rs
- crates/colony-exec/src/buffers/view.rs
- crates/colony-exec/src/goals/tree.rs
- crates/colony-exec/src/sidebar.rs
- crates/colony-exec/src/special_files/prompt_md.rs
- crates/colony-filetree/src/state.rs
- crates/colony-focus/src/error.rs
- crates/colony-memory/src/context.rs
- crates/colony-memory/src/query.rs
- crates/colony-panel/src/host.rs
- crates/colony-panel/src/layout.rs
- crates/colony-panel/src/protocol.rs
- crates/colony-parser/src/ast.rs
- crates/colony-recipe/src/composer.rs
- crates/colony-recipe/src/version.rs
- crates/colony-reflection/src/storage.rs
- crates/colony-shell/src/event_worker.rs
- crates/colony-shell/src/tutorial/explain/commands.rs
- crates/colony-shell/src/tutorial/teach.rs
- crates/colony-shell/src/tutorial/tui/controls.rs
- crates/colony-tui/src/editor/highlight.rs
- crates/colony-tui/src/editor/keybindings.rs
- crates/colony-tuple/src/persistent.rs
- crates/colony-tuple/src/transaction.rs
- crates/colony-ui-core/src/component.rs
- crates/colony-agent/src/providers/ollama.rs
- crates/colony-channels/src/diag.rs
- crates/colony-channels/src/llm.rs
- crates/colony-exec/src/goals/types.rs
- crates/colony-filedetails-bin/src/main.rs
- crates/colony-focus/src/goal.rs
- crates/colony-gitstage-bin/src/main.rs
- crates/colony-gitstatus-bin/src/main.rs
- crates/colony-jobs-bin/src/main.rs
- crates/colony-parser/examples/parser_demo.rs
- crates/colony-preview-bin/src/main.rs
- crates/colony-secure/src/approvals.rs
- crates/colony-tui/src/input/completion.rs
- crates/colony-tuple/src/ipc.rs
- crates/colony-ui-core/src/builtins/git_branch.rs
- crates/colony-ui-core/src/builtins/git_status.rs
- crates/colony-ui-core/src/builtins/node_version.rs
- crates/colony-ui-core/src/sidebar/panels/file_tree.rs
- crates/colony-ui-core/src/sidebar/panels/mcp.rs
- crates/colony-tui/src/app.rs
- crates/colony-agent/src/agent.rs
- crates/colony-agent/src/persistence.rs
- crates/colony-agent/src/router.rs
- crates/colony-agent/src/tool.rs
- crates/colony-exec/src/buffers/span.rs
- crates/colony-exec/src/file_tree/protocol.rs
- crates/colony-exec/src/mirror.rs
- crates/colony-exec/src/special_files/agent_md.rs
- crates/colony-filetree/src/protocol.rs
- crates/colony-focus/src/persistence.rs
- crates/colony-focus/src/tree.rs
- crates/colony-memory/src/store.rs
- crates/colony-tui/src/editor/completion.rs
- crates/colony-tui/src/editor/input.rs
- crates/colony-tuple/src/lib.rs
- crates/colony-exec/examples/reactive_prompt.rs

Mechanical concern signals — navigation aid, not scoring evidence:
Confirm or refute each with your own code reading. Report only confirmed defects.
  - [design_concern] crates/colony-agent/src/planning.rs
    summary: Design signals from orphaned, structural
    question: Is this file truly dead, or is it used via a non-import mechanism (dynamic import, CLI entry point, plugin)?
    evidence: Flagged by: orphaned, structural
    evidence: File size: 575 lines
  - [design_concern] crates/colony-agent/src/provider_chain.rs
    summary: Design signals from orphaned, signature
    question: Is this file truly dead, or is it used via a non-import mechanism (dynamic import, CLI entry point, plugin)?
    evidence: Flagged by: orphaned, signature
    evidence: [signature] 'complete' has 4 different signatures across 9 files
  - [design_concern] crates/colony-agent/src/providers/cloud.rs
    summary: Design signals from orphaned, signature
    question: Is this file truly dead, or is it used via a non-import mechanism (dynamic import, CLI entry point, plugin)?
    evidence: Flagged by: orphaned, signature
    evidence: [signature] 'from_env' has 4 different signatures across 5 files
  - [design_concern] crates/colony-channels/src/protocol.rs
    summary: Design signals from orphaned, structural
    question: Is this file truly dead, or is it used via a non-import mechanism (dynamic import, CLI entry point, plugin)?
    evidence: Flagged by: orphaned, structural
    evidence: File size: 522 lines
  - [design_concern] crates/colony-cloud-sdk/src/timeout.rs
    summary: Design signals from orphaned, signature
    question: Is this file truly dead, or is it used via a non-import mechanism (dynamic import, CLI entry point, plugin)?
    evidence: Flagged by: orphaned, signature
    evidence: [signature] 'start' has 4 different signatures across 4 files
  - [design_concern] crates/colony-config/src/trust.rs
    summary: Design signals from orphaned, signature
    question: Is this file truly dead, or is it used via a non-import mechanism (dynamic import, CLI entry point, plugin)?
    evidence: Flagged by: orphaned, signature
    evidence: [signature] 'save' has 2 different signatures across 4 files
  - [design_concern] crates/colony-exec/src/buffers/buffer.rs
    summary: Design signals from orphaned, signature
    question: Is this file truly dead, or is it used via a non-import mechanism (dynamic import, CLI entry point, plugin)?
    evidence: Flagged by: orphaned, signature
    evidence: [signature] 'push' has 3 different signatures across 3 files
  - [design_concern] crates/colony-exec/src/buffers/view.rs
    summary: Design signals from orphaned, signature
    question: Is this file truly dead, or is it used via a non-import mechanism (dynamic import, CLI entry point, plugin)?
    evidence: Flagged by: orphaned, signature
    evidence: [signature] 'scroll_down' has 5 different signatures across 10 files
  - (+4 more concern signals)

Task requirements:
1. Read the blind packet's `system_prompt` — it contains scoring rules and calibration.
2. Start from the seed files, then freely explore the repository to build your understanding.
3. Keep issues and scoring scoped to this batch's dimension.
4. Respect scope controls: do not include files/directories marked by `exclude`, `suppress`, or non-production zone overrides.
5. Return 0-10 issues for this batch (empty array allowed).
6. For design_coherence, use evidence from `holistic_context.scan_evidence.signal_density` — files where multiple mechanical detectors fired. Investigate what design change would address multiple signals simultaneously. Check `scan_evidence.complexity_hotspots` for files with high responsibility cluster counts.
7. Workflow integrity checks: when reviewing orchestration/queue/review flows,
8. xplicitly look for loop-prone patterns and blind spots:
9. - repeated stale/reopen churn without clear exit criteria or gating,
10. - packet/batch data being generated but dropped before prompt execution,
11. - ranking/triage logic that can starve target-improving work,
12. - reruns happening before existing open review work is drained.
13. If found, propose concrete guardrails and where to implement them.
14. Complete `dimension_judgment` for your dimension — all three fields (strengths, issue_character, score_rationale) are required. Write the judgment BEFORE setting the score.
15. Do not edit repository files.
16. Return ONLY valid JSON, no markdown fences.

Scope enums:
- impact_scope: "local" | "module" | "subsystem" | "codebase"
- fix_scope: "single_edit" | "multi_file_refactor" | "architectural_change"

Output schema:
{
  "batch": "design_coherence",
  "batch_index": 15,
  "assessments": {"<dimension>": <0-100 with one decimal place>},
  "dimension_notes": {
    "<dimension>": {
      "evidence": ["specific code observations"],
      "impact_scope": "local|module|subsystem|codebase",
      "fix_scope": "single_edit|multi_file_refactor|architectural_change",
      "confidence": "high|medium|low",
      "issues_preventing_higher_score": "required when score >85.0",
      "sub_axes": {"abstraction_leverage": 0-100, "indirection_cost": 0-100, "interface_honesty": 0-100, "delegation_density": 0-100, "definition_directness": 0-100, "type_discipline": 0-100}  // required for abstraction_fitness when evidence supports it; all one decimal place
    }
  },
  "dimension_judgment": {
    "<dimension>": {
      "strengths": ["0-5 specific things the codebase does well from this dimension's perspective"],
      "issue_character": "one sentence characterizing the nature/pattern of issues from this dimension's perspective",
      "score_rationale": "2-3 sentences explaining the score from this dimension's perspective, referencing global anchors"
    }  // required for every assessed dimension; do not omit
  },
  "issues": [{
    "dimension": "<dimension>",
    "identifier": "short_id",
    "summary": "one-line defect summary",
    "related_files": ["relative/path.py"],
    "evidence": ["specific code observation"],
    "suggestion": "concrete fix recommendation",
    "confidence": "high|medium|low",
    "impact_scope": "local|module|subsystem|codebase",
    "fix_scope": "single_edit|multi_file_refactor|architectural_change",
    "root_cause_cluster": "optional_cluster_name_when_supported_by_history"
  }],
  "retrospective": {
    "root_causes": ["optional: concise root-cause hypotheses"],
    "likely_symptoms": ["optional: identifiers that look symptom-level"],
    "possible_false_positives": ["optional: prior concept keys likely mis-scoped"]
  }
}
