<script lang="ts">
	import { devMode } from '$lib/stores/settings';

	let open = false;

	function toggle() {
		open = !open;
	}

	function handleClickOutside(event: MouseEvent) {
		const target = event.target as HTMLElement;
		if (!target.closest('.settings-container')) {
			open = false;
		}
	}
</script>

<svelte:window on:click={handleClickOutside} />

<div class="settings-container">
	<button class="settings-btn" on:click|stopPropagation={toggle} title="Settings">
		<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
			<circle cx="12" cy="12" r="3"></circle>
			<path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"></path>
		</svg>
	</button>

	{#if open}
		<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
		<div class="settings-panel" on:click|stopPropagation>
			<div class="settings-header">Settings</div>
			<label class="setting-item">
				<input type="checkbox" bind:checked={$devMode} />
				<span>Developer Mode</span>
			</label>
			<p class="setting-desc">Show detailed performance metrics (tokens/sec, eval time)</p>
		</div>
	{/if}
</div>

<style>
	.settings-container {
		position: relative;
		margin-left: 0.5rem;
	}

	.settings-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 0.25rem;
		border: none;
		background: transparent;
		color: var(--text);
		cursor: pointer;
		opacity: 0.7;
		transition: opacity 0.2s;
	}

	.settings-btn:hover {
		opacity: 1;
	}

	.settings-panel {
		position: absolute;
		top: 100%;
		right: 0;
		margin-top: 0.5rem;
		padding: 0.75rem;
		background: var(--bg-secondary);
		border: 1px solid var(--border);
		border-radius: 6px;
		min-width: 200px;
		z-index: 100;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
	}

	.settings-header {
		font-weight: 600;
		margin-bottom: 0.75rem;
		padding-bottom: 0.5rem;
		border-bottom: 1px solid var(--border);
	}

	.setting-item {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		cursor: pointer;
	}

	.setting-item input {
		cursor: pointer;
	}

	.setting-desc {
		margin: 0.25rem 0 0 1.5rem;
		font-size: 0.75rem;
		opacity: 0.7;
	}
</style>
