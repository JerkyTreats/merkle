# Research Terms and Foundations

Date: 2026-02-28.

## Purpose

This document maps terminology and architecture concepts to primary references, then applies them to this repository.

## Recommended Vocabulary

| Candidate term | Recommended use | Reason |
| --- | --- | --- |
| workload | Reserved for infrastructure compute load | Kubernetes defines workload as an application running on a platform |
| job | One run to completion unit | Kubernetes Job docs define this as one off completion work |
| workflow | Definition of orchestrated multi step process | Temporal and Step Functions both define durable orchestrated flow |
| workflow run | One execution instance of a workflow definition | Temporal differentiates workflow definition from workflow execution |
| action | Side effecting step such as write emit notify | Clear domain term for effectful operations |
| policy decision | Decision that allows denies defers action | OPA model separates decision from enforcement |
| trigger event | Input signal that may start a workflow run | Event based systems use explicit event boundaries |
| provenance | Trace of who did what when and from which inputs | W3C PROV provides standard core model |

## Foundational Findings

1. Workflow definition versus workflow run is a strong distinction
- Temporal states that workflow definition is code while workflow execution is the running instance.
- This maps directly to a design split between immutable definition and mutable run state.

2. Determinism and idempotency are separate concerns
- Temporal requires deterministic workflow behavior for replay safety.
- Step Functions distinguishes exactly once and at least once semantics by workflow type.
- Design implication: policy and orchestration logic must be deterministic, while actions must be idempotent.

3. Execution identity is a first class control for duplicate suppression
- Step Functions StartExecution provides idempotent behavior for standard workflow with same name and input.
- Design implication: every run should carry a deterministic run key and reject duplicate starts.

4. Policy decision and action enforcement should be split
- OPA describes policy decision point and policy enforcement point separation.
- Design implication: reflection engine should decide, while an action engine should execute.

5. Run lifecycle should use explicit states with terminal boundaries
- OpenLineage run cycle defines start running complete abort fail and terminal behavior.
- Design implication: workflow run state machine should prevent late events after terminal state.

6. Provenance model should include entity activity agent
- W3C PROV frames provenance as entities activities and agents.
- Design implication: represent workflow input artifacts as entities, workflow runs as activities, and agent plus provider as agents.

7. Merkle structures support immutable traceable activation graphs
- Git data model and IPFS Merkle DAG both rely on immutable content addressed nodes.
- Design implication: workflow definitions and run records can be content addressed and linked without mutation.

8. Workload is not the best main term for this feature
- Kubernetes uses workload for deployed application resource group.
- Your concept is better represented as workflow plus workflow run plus action.

## Term Set for This Repository

Use these terms in specs and future code:
- `workflow_definition` as immutable orchestration plan
- `workflow_run` as one execution instance
- `trigger_event` as event payload from context or watch
- `policy_decision` as output of reflection engine
- `action_invocation` as side effecting command request
- `action_result` as durable run output
- `run_provenance` as trace metadata and lineage

## Source Links

- [Temporal Workflow Definition](https://docs.temporal.io/workflow-definition)
- [Temporal Workflow Execution Overview](https://docs.temporal.io/workflow-execution)
- [AWS Step Functions StartExecution API](https://docs.aws.amazon.com/step-functions/latest/apireference/API_StartExecution.html)
- [AWS Step Functions Workflow Type and Execution Guarantees](https://docs.aws.amazon.com/step-functions/latest/dg/choosing-workflow-type.html)
- [AWS Step Functions Nested Workflows](https://docs.aws.amazon.com/step-functions/latest/dg/concepts-nested-workflows.html)
- [Open Policy Agent Docs](https://www.openpolicyagent.org/docs)
- [Open Policy Agent Deployment Model](https://www.openpolicyagent.org/docs/deploy)
- [OpenLineage Run Cycle](https://openlineage.io/docs/spec/run-cycle/)
- [W3C PROV Overview](https://www.w3.org/TR/prov-overview/)
- [W3C PROV Primer](https://www.w3.org/TR/prov-primer/)
- [Kubernetes Workloads](https://kubernetes.io/docs/concepts/workloads/)
- [Kubernetes Jobs](https://kubernetes.io/docs/concepts/workloads/controllers/job/)
- [Git Core Data Model](https://git-scm.com/docs/gitdatamodel.html)
- [IPFS Merkle DAG Concept](https://docs.ipfs.tech/concepts/merkle-dag/)

## Inference Notes

The separation of deterministic orchestration from idempotent actions is an inference from combining Temporal replay constraints with Step Functions execution semantics.
