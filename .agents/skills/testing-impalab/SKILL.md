---
name: testing-impalab
description: Principles and workflows for identifying, updating, and adding high-value unit and integration tests. Use this skill when modifying core logic, adding new orchestration features, or addressing issues in data stream integrity and configuration resolution.
---

# Testing Impalab

Ensure the reliability of the benchmarking orchestrator by combining granular unit tests for internal logic with end-to-end integration tests for process management.

## Integration Testing Principles

Focus integration tests on the "process-boundary" logic where the orchestrator interacts with the operating system and external binaries.

### Execution Workflow
1. **Isolated Workspace**: Initialize an empty, temporary directory for every test to prevent cross-contamination.
2. **Fixture Materialization**: Place valid component definitions (manifests and scripts) within the workspace to simulate a real project environment.
3. **Binary Invocation**: Execute the primary tool binary against the workspace using standardized argument patterns.
4. **Behavioral Assertion**:
    * Validate the exit status matches the expected outcome (success or specific error category).
    * Use string predicates to verify that critical progress milestones or error descriptions appear in the output streams.

### Testing for Determinism
* **Environment Sanitization**: Force consistent output by disabling terminal styling (e.g., via environment variables) to ensure predictable string matching.

## High-Value Unit Testing

Prioritize unit tests for internal modules where logic complexity is high and process overhead is unnecessary.

### 1. Configuration Resolution & Merging
Verify the hierarchy of configuration sources:
* **Precedence Logic**: Ensure that high-priority inputs (like direct command-line overrides) correctly supersede lower-priority sources (like project-level manifests).
* **Identity Transition**: Confirm that switching the identity of a core component (e.g., a data source or worker) triggers the discarding of now-irrelevant parameters from lower layers.
* **Validation Guards**: Test that invalid configurations (such as type mismatches or references to missing components) are caught during the "resolve" phase before any processes are spawned.

### 2. Data Parsing & Stream Protocols
Test the logic that interprets results from external workers:
* **Protocol Compliance**: Verify the parser correctly extracts identifiers and numeric metrics from standardized string formats.
* **Robustness**: Ensure the orchestrator remains stable when encountering unexpected output formats, empty lines, or truncated data streams.

### 3. Dependency Graphing
* **Component Discovery**: Test the logic that maps relative paths and identifies available workers within a project structure.

## Skill Standards
* **Declarative Intent**: Focus on *what* should be verified (e.g., "verify precedence") rather than specific variable names.
* **Failure Clarity**: When adding tests, ensure they provide enough context (e.g., through log levels or descriptive assertions) to identify which layer of the system failed.
* **Clean State**: Ensure tests clean up resources or use volatile storage to remain idempotent.