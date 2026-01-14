<script lang="ts">
	import Settings from './Settings.svelte';
	import type { ModelConfig } from '$lib/types';

	export let isConnected: boolean;
	export let models: ModelConfig[];
	export let selectedModel: string;
	export let modelStatus: string;

	$: statusText = modelStatus === 'loading' ? 'Loading model...' :
		modelStatus === 'unloading' ? 'Unloading model...' : '';
</script>

<header>
	<div class="status" class:connected={isConnected}></div>
	<b>agents-rs</b>
	{#if statusText}
		<span class="model-status">{statusText}</span>
	{/if}
	<select bind:value={selectedModel} class="model-select" class:no-status={!statusText} disabled={!isConnected || !!statusText}>
		<option value="none">-- Unload GPU --</option>
		{#each models as model}
			<option value={model.id}>{model.name}</option>
		{/each}
	</select>
	<Settings />
</header>

<style>
	.model-status {
		margin-left: auto;
		font-size: 0.875rem;
		color: var(--text-secondary, #888);
		font-style: italic;
	}

	.model-select {
		margin-left: 0.5rem;
		padding: 0.25rem 0.5rem;
		border-radius: 4px;
		border: 1px solid var(--border);
		background: var(--bg-secondary);
		color: var(--text);
		font-size: 0.875rem;
		cursor: pointer;
	}

	.model-select:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.model-select.no-status {
		margin-left: auto;
	}
</style>
