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
- crates/colony-env-bin/src/main.rs
- crates/colony-shell/src/repl.rs
- crates/colony-exec/src/flux/mod.rs
- crates/colony-exec/src/mirror.rs
- crates/colony-ui-core/src/sidebar/panels/file_tree.rs
- crates/colony-protocol/src/lib.rs
- crates/colony-filetree-bin/src/main.rs
- crates/colony-tui/src/editor/buffer.rs
- crates/colony-agent/examples/simple_agent.rs
- crates/colony-agent/src/approval.rs
- crates/colony-agent/src/cancellation.rs
- crates/colony-agent/src/context.rs
- crates/colony-agent/src/detector.rs
- crates/colony-agent/src/lib.rs
- crates/colony-agent/src/memory_integration.rs
- crates/colony-agent/src/planning.rs
- crates/colony-agent/src/provider.rs
- crates/colony-agent/src/provider_chain.rs
- crates/colony-agent/src/providers/anthropic.rs
- crates/colony-agent/src/providers/cloud.rs
- crates/colony-agent/src/providers/mod.rs
- crates/colony-agent/src/providers/openai.rs
- crates/colony-agent/src/recipe_config.rs
- crates/colony-agent/src/token_accounting.rs
- crates/colony-agent/src/tuple_bridge.rs
- crates/colony-channels/src/buffer.rs
- crates/colony-channels/src/channel.rs
- crates/colony-channels/src/lib.rs
- crates/colony-channels/src/manager.rs
- crates/colony-channels/src/pipe.rs
- crates/colony-channels/src/protocol.rs
- crates/colony-channels/src/tool.rs
- crates/colony-cloud-sdk/src/client.rs
- crates/colony-cloud-sdk/src/config.rs
- crates/colony-cloud-sdk/src/lib.rs
- crates/colony-cloud-sdk/src/models.rs
- crates/colony-cloud-sdk/src/rate_limit.rs
- crates/colony-cloud-sdk/src/retry.rs
- crates/colony-cloud-sdk/src/streaming.rs
- crates/colony-cloud-sdk/src/timeout.rs
- crates/colony-config/src/discovery.rs
- crates/colony-config/src/lib.rs
- crates/colony-config/src/trust.rs
- crates/colony-exec/src/alias.rs
- crates/colony-exec/src/buffers/ansi.rs
- crates/colony-exec/src/buffers/buffer.rs
- crates/colony-exec/src/buffers/mod.rs
- crates/colony-exec/src/buffers/search.rs
- crates/colony-exec/src/buffers/view.rs
- crates/colony-exec/src/builtin_redirect.rs
- crates/colony-exec/src/file_tree/git_status.rs
- crates/colony-exec/src/file_tree/mod.rs
- crates/colony-exec/src/file_tree/tree_state.rs
- crates/colony-exec/src/file_tree/watcher.rs
- crates/colony-exec/src/goals/mod.rs
- crates/colony-exec/src/goals/tree.rs
- crates/colony-exec/src/jobs.rs
- crates/colony-exec/src/panes/builtins.rs
- crates/colony-exec/src/panes/layout.rs
- crates/colony-exec/src/panes/mod.rs
- crates/colony-exec/src/path.rs
- crates/colony-exec/src/process.rs
- crates/colony-exec/src/sidebar.rs
- crates/colony-exec/src/special_files/context_md.rs
- crates/colony-exec/src/special_files/heartbeat.rs
- crates/colony-exec/src/special_files/mod.rs
- crates/colony-exec/src/special_files/prompt_md.rs
- crates/colony-exec/src/special_files/tools_md.rs
- crates/colony-exec/src/special_files/types.rs
- crates/colony-exec/src/xtrace.rs
- crates/colony-expand/src/brace.rs
- crates/colony-expand/src/field_split.rs
- crates/colony-expand/src/tilde.rs
- crates/colony-filetree/src/git.rs
- crates/colony-filetree/src/lib.rs
- crates/colony-filetree/src/node.rs
- crates/colony-filetree/src/state.rs
- crates/colony-filetree/src/watcher.rs
- crates/colony-focus/src/error.rs
- crates/colony-focus/src/lib.rs
- crates/colony-logs-bin/src/main.rs
- crates/colony-mcp/src/codegen.rs
- crates/colony-mcp/src/connection.rs
- crates/colony-mcp/src/lib.rs
- crates/colony-mcp/src/pool.rs
- crates/colony-mcp/src/session.rs
- crates/colony-memory/src/context.rs
- crates/colony-memory/src/decay.rs

Mechanical concern signals — navigation aid, not scoring evidence:
Confirm or refute each with your own code reading. Report only confirmed defects.
  - [design_concern] crates/colony-agent/examples/simple_agent.rs
    summary: Design signals from orphaned
    question: Is this file truly dead, or is it used via a non-import mechanism (dynamic import, CLI entry point, plugin)?
    evidence: Flagged by: orphaned
    evidence: [orphaned] Orphaned file (30 LOC): zero importers, not an entry point
  - [design_concern] crates/colony-agent/src/approval.rs
    summary: Design signals from orphaned
    question: Is this file truly dead, or is it used via a non-import mechanism (dynamic import, CLI entry point, plugin)?
    evidence: Flagged by: orphaned
    evidence: [orphaned] Orphaned file (340 LOC): zero importers, not an entry point
  - [design_concern] crates/colony-agent/src/cancellation.rs
    summary: Design signals from orphaned
    question: Is this file truly dead, or is it used via a non-import mechanism (dynamic import, CLI entry point, plugin)?
    evidence: Flagged by: orphaned
    evidence: [orphaned] Orphaned file (242 LOC): zero importers, not an entry point
  - [design_concern] crates/colony-agent/src/context.rs
    summary: Design signals from orphaned
    question: Is this file truly dead, or is it used via a non-import mechanism (dynamic import, CLI entry point, plugin)?
    evidence: Flagged by: orphaned
    evidence: [orphaned] Orphaned file (368 LOC): zero importers, not an entry point
  - [design_concern] crates/colony-agent/src/detector.rs
    summary: Design signals from orphaned
    question: Is this file truly dead, or is it used via a non-import mechanism (dynamic import, CLI entry point, plugin)?
    evidence: Flagged by: orphaned
    evidence: [orphaned] Orphaned file (94 LOC): zero importers, not an entry point
  - [design_concern] crates/colony-agent/src/lib.rs
    summary: Design signals from orphaned
    question: Is this file truly dead, or is it used via a non-import mechanism (dynamic import, CLI entry point, plugin)?
    evidence: Flagged by: orphaned
    evidence: [orphaned] Orphaned file (74 LOC): zero importers, not an entry point
  - [design_concern] crates/colony-agent/src/memory_integration.rs
    summary: Design signals from orphaned
    question: Is this file truly dead, or is it used via a non-import mechanism (dynamic import, CLI entry point, plugin)?
    evidence: Flagged by: orphaned
    evidence: [orphaned] Orphaned file (382 LOC): zero importers, not an entry point
  - [design_concern] crates/colony-agent/src/planning.rs
    summary: Design signals from orphaned, structural
    question: Is this file truly dead, or is it used via a non-import mechanism (dynamic import, CLI entry point, plugin)?
    evidence: Flagged by: orphaned, structural
    evidence: File size: 575 lines
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
