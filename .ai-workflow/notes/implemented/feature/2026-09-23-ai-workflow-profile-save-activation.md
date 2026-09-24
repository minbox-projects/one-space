# Agent Note: AI Workflow Profile Save and Activation Are Separate Actions

Status: implemented

English | [中文](2026-09-23-ai-workflow-profile-save-activation.zh.md)

## Problem

The AI Workflow model switcher originally combined saving a profile and activating it, which meant that a user intending only to persist edits to an existing profile could also change the active profile. UI-created profiles also needed a clear first-save decision without asking before the profile matrix had been persisted.

## Decision

Saving and activation are distinct actions. Save persists the selected matrix without changing the active profile; Activate operates on the profile's saved YAML through the existing `ai_workflow_activate_profile` path. For an existing profile, Save never prompts. For a profile created in the UI, the first successful Save is followed by a Yes/No prompt: No leaves the profile saved and inactive, while Yes separately activates the now-persisted YAML. A failed Save never prompts. If the separate activation fails, the saved matrix remains persisted.

The UI behavior is implemented in `src/components/AiWorkflowModelSwitcher/AiWorkflowModelSwitcher.tsx`; persistence uses the new `ai_workflow_save_profile` command, while activation uses the existing `ai_workflow_activate_profile` command. The existing combined `ai_workflow_save_and_activate_profile` command, including its snapshot rollback behavior, remains available for compatibility.

## Alternatives considered

- Keep Save and Activate combined: declined because saving edits to an existing profile could unintentionally switch the active profile.
- Save first, then prompt whether to activate: selected because persistence completes before the user makes an explicit activation choice, and No retains a saved inactive profile.
- Prompt before saving: declined because it could ask the user to activate a matrix that had not yet been persisted.

## Consequences

- Existing-profile Save is a persistence-only operation and does not change active-profile state; Activate remains an explicit operation over saved YAML.
- A UI-created profile prompts once, and only after its first successful save. Save failure cannot lead to an activation prompt or activation attempt.
- Activation is subsequent to persistence. An activation failure does not discard the successfully saved matrix; the combined backend command and its rollback remain available but are not used for these separated UI actions.
- The broader switcher implementation and its original alternatives remain documented in [AI Workflow Model Switcher Delivers 9-by-3 Matrix with Backend Profile Commands](2026-09-22-ai-workflow-model-switcher.md).
