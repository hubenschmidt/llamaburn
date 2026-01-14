<script lang="ts">
	import type { WsMetadata } from '$lib/types';

	export let user: 'User' | 'Bot';
	export let msg: string;
	export let streaming: boolean = false;
	export let metadata: WsMetadata | undefined = undefined;

	function formatMetadata(m: WsMetadata): string {
		const secs = (m.elapsed_ms / 1000).toFixed(1);

		if (m.tokens_per_sec !== undefined) {
			return `${secs}s · ${m.tokens_per_sec.toFixed(1)} tok/s · ${m.output_tokens} tokens`;
		}

		if (m.input_tokens > 0 || m.output_tokens > 0) {
			return `${secs}s · ${m.input_tokens}/${m.output_tokens} tokens`;
		}

		return `${secs}s`;
	}
</script>

<div
	class="message"
	class:user={user === 'User'}
	class:bot={user === 'Bot'}
	class:streaming
>
	{msg}
	{#if user === 'Bot' && metadata && !streaming}
		<div class="metadata">{formatMetadata(metadata)}</div>
	{/if}
</div>
