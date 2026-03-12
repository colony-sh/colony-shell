You are a focused subagent reviewer for a single holistic investigation batch.

Repository root: /Users/mcfearsome/src/github.com/colony-sh/colony-shell
Blind packet: /Users/mcfearsome/src/github.com/colony-sh/colony-shell/.desloppify/review_packet_blind.json
Batch index: 20
Batch name: api_surface_coherence
Batch rationale: no direct batch mapping for api_surface_coherence; using representative files

DIMENSION TO EVALUATE:

## api_surface_coherence
Inconsistent API shapes, mixed sync/async, overloaded interfaces
Look for:
- Inconsistent API shapes: similar functions with different parameter ordering or naming
- Mixed sync/async in the same module's public API
- Overloaded interfaces: one function doing too many things based on argument types
- Missing error contracts: no documentation or types indicating what can fail
- Public functions with >5 parameters (API boundary may be wrong)
Skip:
- Internal/private APIs where flexibility is acceptable
- Framework-imposed patterns (React hooks must follow rules of hooks)

YOUR TASK: Read the code for this batch's dimension. Judge how well the codebase serves a developer from that perspective. The dimension rubric above defines what good looks like. Cite specific observations that explain your judgment.

Mechanical scan evidence — navigation aid, not scoring evidence:
The blind packet contains `holistic_context.scan_evidence` with aggregated signals from all mechanical detectors — including complexity hotspots, error hotspots, signal density index, boundary violations, and systemic patterns. Use these as starting points for where to look beyond the seed files.

Seed files (start here):
- crates/colony-agent/src/persistence.rs
- crates/colony-agent/src/providers/anthropic.rs
- crates/colony-agent/src/providers/cloud.rs
- crates/colony-agent/src/providers/ollama.rs
- crates/colony-exec/src/redirect.rs
- crates/colony-focus/src/persistence.rs
- crates/colony-memory/src/store.rs
- crates/colony-protocol/src/tree.rs
- crates/colony-recipe/src/registry.rs
- crates/colony-reflection/src/storage.rs
- crates/colony-secure/src/approvals.rs
- crates/colony-ui-core/src/builtins/aws_profile.rs
- crates/colony-ui-core/src/builtins/docker_status.rs
- crates/colony-ui-core/src/builtins/duration.rs
- crates/colony-agent/src/agent.rs
- crates/colony-agent/src/approval.rs
- crates/colony-agent/src/cancellation.rs
- crates/colony-exec/src/alias.rs
- crates/colony-exec/src/builtin_redirect.rs
- crates/colony-exec/src/builtins.rs
- crates/colony-ui-core/src/builtins/lsp.rs
- crates/colony-tui/src/prompt/renderer.rs
- crates/colony-ui-core/src/sidebar/panels/file_tree.rs
- crates/colony-env-bin/src/main.rs
- crates/colony-ui-core/src/builtins/git_branch.rs
- crates/colony-ui-core/src/builtins/git_status.rs
- crates/colony-channels/src/diag.rs
- crates/colony-ui-core/src/builtins/node_version.rs
- crates/colony-filetree-bin/src/main.rs
- crates/colony-tuple/src/ipc.rs
- crates/colony-agent/src/recipe_config.rs
- crates/colony-expand/src/context.rs
- crates/colony-exec/src/history.rs
- crates/colony-mcp/src/connection.rs
- crates/colony-shell/src/repl.rs
- crates/colony-ui-core/src/sidebar/manager.rs
- crates/colony-recipe/src/composer.rs
- crates/colony-agent/src/tuple_bridge.rs
- crates/colony-exec/src/goals/tree.rs
- crates/colony-tui/src/sidebar/renderer.rs
- crates/colony-mcp/src/pool.rs
- crates/colony-recipe/src/recipe.rs
- crates/colony-secure/src/config.rs
- crates/colony-exec/src/special_files/agent_md.rs
- crates/colony-shell/src/tutorial/tracer.rs
- crates/colony-recipe/src/loader.rs
- crates/colony-agent/src/memory_integration.rs
- crates/colony-exec/src/lib.rs
- crates/colony-exec/src/pipeline.rs
- crates/colony-expand/src/arithmetic.rs
- crates/colony-exec/src/command_sub.rs
- crates/colony-expand/src/parameter.rs
- crates/colony-expand/src/lib.rs
- crates/colony-shell/src/shell/orchestrator.rs
- crates/colony-recipe/src/mcp_lifecycle.rs
- crates/colony-channels/src/context.rs
- crates/colony-agent/src/provider_chain.rs
- crates/colony-tuple/src/persistent.rs
- crates/colony-shell/src/tutorial/runner.rs
- crates/colony-shell/src/shell/execution/control_flow.rs
- crates/colony-shell/src/tutorial/tui/magic_pane.rs
- crates/colony-shell/src/shell/execution/serialization.rs
- crates/colony-shell/src/repl_tui.rs
- crates/colony-shell/src/tutorial/explain/constructs.rs
- crates/colony-shell/src/tutorial/explain/commands.rs
- crates/colony-tui/src/editor/input.rs
- crates/colony-filetree/src/protocol.rs
- crates/colony-tui/src/app.rs
- crates/colony-shell/src/shell/engine.rs
- crates/colony-shell/src/tutorial/tui/script_pane.rs
- crates/colony-exec/src/mirror.rs
- crates/colony-agent/src/lib.rs
- crates/colony-agent/src/providers/mod.rs
- crates/colony-channels/src/channel.rs
- crates/colony-channels/src/lib.rs
- crates/colony-cloud-sdk/src/error.rs
- crates/colony-cloud-sdk/src/lib.rs
- crates/colony-cloud-sdk/src/models.rs
- crates/colony-exec/src/buffers/mod.rs
- crates/colony-exec/src/error.rs

Task requirements:
1. Read the blind packet's `system_prompt` — it contains scoring rules and calibration.
2. Start from the seed files, then freely explore the repository to build your understanding.
3. Keep issues and scoring scoped to this batch's dimension.
4. Respect scope controls: do not include files/directories marked by `exclude`, `suppress`, or non-production zone overrides.
5. Return 0-10 issues for this batch (empty array allowed).
6. Complete `dimension_judgment` for your dimension — all three fields (strengths, issue_character, score_rationale) are required. Write the judgment BEFORE setting the score.
7. Do not edit repository files.
8. Return ONLY valid JSON, no markdown fences.

Scope enums:
- impact_scope: "local" | "module" | "subsystem" | "codebase"
- fix_scope: "single_edit" | "multi_file_refactor" | "architectural_change"

Output schema:
{
  "batch": "api_surface_coherence",
  "batch_index": 20,
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
