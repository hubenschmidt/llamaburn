<script lang="ts">
	import Settings from './Settings.svelte';
	import type { ModelConfig } from '$lib/types';

	export let isConnected: boolean;
	export let models: ModelConfig[];
	export let selectedModel: string;
</script>

<header>
	<div class="status" class:connected={isConnected}></div>
	<b>agents-rs</b>
	<select bind:value={selectedModel} class="model-select" disabled={!isConnected}>
		{#each models as model}
			<option value={model.id}>{model.name}</option>
		{/each}
	</select>
	<Settings />
</header>

<style>
	.model-select {
		margin-left: auto;
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
</style>
