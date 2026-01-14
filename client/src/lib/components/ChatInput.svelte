<script lang="ts">
	export let value: string;
	export let disabled: boolean = false;
	export let sendDisabled: boolean = false;
	export let onSend: () => void;

	function handleKeydown(event: KeyboardEvent) {
		if (event.key !== 'Enter') return;
		if (event.shiftKey) return;
		event.preventDefault();
		onSend();
	}
</script>

<div class="input-area">
	<textarea
		bind:value
		onkeydown={handleKeydown}
		placeholder="Type a message..."
		{disabled}
		rows="1"
	></textarea>
	<button onclick={onSend} disabled={sendDisabled}>
		Send
	</button>
</div>
