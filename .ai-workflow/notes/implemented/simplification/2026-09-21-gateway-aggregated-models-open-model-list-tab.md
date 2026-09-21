# Agent Note: API Gateway Aggregated Models Open the Model List Tab Instead of a Dialog

Status: implemented

English | [中文](2026-09-21-gateway-aggregated-models-open-model-list-tab.zh.md)

## Problem

The `Aggregated models` metric card on the `api-gateway` page opened `src/components/ApiGateway/AggregatedModelsDialog.tsx`, a second surface over the same `aggregateModels` source and the same provider and mapping predicate as the `Model list` tab. The dialog listed every served model at once, offered no search, and closed with the operator's next click, so it duplicated a surface that is one tab click away.

## Decision

`src/components/ApiGateway/AggregatedModelsDialog.tsx` was removed together with the page's `isModelsDialogOpen` state and the card's optional `onShowModels` prop, and the `Model list` tab is now the single aggregated-models list surface. `src/components/ApiGateway/RuntimeStatusCard.tsx` calls the existing optional `onSelectTab?.("models")` from both the metric card's click handler and its Enter/Space keydown handler, so the card switches the page to the `Model list` tab instead of opening a dialog.

The shared-count guarantee is unchanged: the card still derives its number with `aggregateModels(config.providers).length`, the tab badge uses the same call, and `src/components/ApiGateway/ModelListPanel.tsx` renders the same `aggregateModels` result, so the count and the list cannot disagree.

The dialog-era copy keys stay where they are still used: `ModelListPanel.tsx` renders `apiGatewayAggregatedModelsDialogDesc` as its empty-state description and `apiGatewayAggregatedModelDefaultBadge` for a default-model source badge, so their legacy names are kept. `apiGatewayAggregatedModelsEmpty`, added for the removed dialog, has no code reference after the removal and remains in `src/i18n.ts` in both languages.

## Alternatives considered

- Keep the dialog and the tab coexisting: declined because both surfaces render the same `aggregateModels` result and the tab already offers search and persistent visibility, so the dialog added only a second presentation of one list.
- Keep the dialog while changing only the metric card's target: declined for the same duplication and to avoid maintaining two open/close paths for one aggregation.
- The [model list tab record](../feature/2026-09-18-api-gateway-model-list-tab.md) had declined replacing or removing the dialog at the time because the card kept its click-through overview without switching tabs; the later search and persistent-visibility decision made that coexistence redundant, so this record adopts the removal it set aside.

## Consequences

- Clicking the `Aggregated models` metric card, or pressing Enter or Space while it is focused, selects the `Model list` tab and opens no dialog; `onSelectTab` stays optional, so the card still renders as a plain card when the prop is absent.
- The shared count and predicate are unchanged: the card number and the tab badge both come from `aggregateModels(config.providers).length`, and `aggregateModels` still contributes only providers whose `enabled` is true and mappings that are `enabled && !auto_disabled`.
- Backend behavior, `GET /v1/models`, terminal sync, session affinity, usage stats and request logs are untouched, and no navigation id, launcher entry or tab order changed.
- `apiGatewayAggregatedModelsDialogDesc` and `apiGatewayAggregatedModelDefaultBadge` remain in use by the model list panel under their dialog-era names, and `apiGatewayAggregatedModelsEmpty` is now unreferenced but retained in `src/i18n.ts`.
- Behavior coverage in `src/components/ApiGateway/ApiGateway.test.tsx` asserts that clicking the metric card, or pressing Enter or Space on it, selects the `Model list` tab and renders the panel; that the rendered model rows equal the number on the card; that the endpoint renders the path form `/chat/completions`; and that a configuration without enabled providers shows the panel's empty state.
- Partial supersession: [API Gateway Aggregated Models Open in a Dialog With a Shared Count](../feature/2026-09-17-api-fusion-aggregated-models-dialog.md) is retained and cross-linked; its shared-aggregation and single-count decisions still stand, while its dialog-surface decision is superseded by this record. [API Gateway Model List Tab Reuses the Shared Aggregation](../feature/2026-09-18-api-gateway-model-list-tab.md) is retained and cross-linked as the record of the tab that becomes the single aggregated-models list surface. [Gateway Per-Model Mapping Disable Is an Explicit Exclusion](../architecture/2026-09-18-gateway-per-mapping-disable.md) and [Gateway Per-Model Auto-Disable Settles Health on the Mapping Row](../architecture/2026-09-20-gateway-per-model-auto-disable.md) record the selection predicate and are unaffected.
