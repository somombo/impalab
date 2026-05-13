---
name: documenting-impalab
description: Use this skill to synchronize README.md, CONTRIBUTING.md, agent skills, and inline code documentation with the latest state of the project.
---

# Instructions

Follow these steps to maintain and update the project's documentation suite and ensure it accurately reflects the current codebase.

## 1. Project Documentation Synchronization
Ensure high-level project files reflect the current operational state.
* **Core Project Overview**: Update the primary project description, setup steps, and basic workflows to match the latest build and run processes.
* **Developer Guidelines**: Align the contributor setup, code style requirements, and submission processes with the current development environment.
* **Agent Skills**: Review all skill files in `.agents/skills/`. Update their descriptions and instruction blocks if the system's capabilities or command structures have changed.

## 2. Inline Code Documentation
Refine technical clarity within the source code using the following standards:
* **Public Interfaces**: Apply standard Docstrings to all public functions, classes, and modules to describe their purpose and expected inputs/outputs.
* **Internal Logic**:
    * Add comments only for non-obvious intent or high-complexity logic.
    * Do not describe standard programming syntax.
* **Housekeeping**: Remove any comments or docstrings that refer to removed components or outdated logic.

## 3. General Principles
* **Declarative Style**: Focus on the "what" and "why" of the system rather than implementation details.
* **Abstract References**: Do not use hardcoded filenames, line numbers, or specific identifiers that are subject to change. Use generic terms like "the build manifest" or "the execution orchestrator."
* **Progressive Disclosure**: Keep instructions concise. Refer to separate reference files within the skill's `references/` directory for deep technical specifications.

## 4. Voice and Tone Standards
Maintain a direct, human-centered tone across all documentation:
* **Vocabulary**: Use simple, everyday words. Avoid "corporate" transition words (e.g., "moreover," "foster," "landscape," "delve").
* **Punctuation and Style**:
    * Do not use em dashes (—); use commas or separate sentences instead.
    * Do not use emojis or hashtags.
* **Directness**: Use active voice and imperative phrasing for instructions.

## 5. Verification
Confirm documentation accuracy before finalizing:
1. Verify that all command-line examples match the current CLI arguments.
2. Check that no contradictory instructions exist between the high-level guides and the specific agent skills.