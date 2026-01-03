<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { chat } from '$lib/stores/chat';

	const WS_URL = 'ws://localhost:8000/ws';

	let inputText = '';
	let messagesContainer: HTMLDivElement;

	onMount(() => {
		chat.connect(WS_URL);
		return () => chat.disconnect();
	});

	async function scrollToBottom() {
		await tick();
		if (messagesContainer) {
			messagesContainer.scrollTop = messagesContainer.scrollHeight;
		}
	}

	$: if ($chat.messages) {
		scrollToBottom();
	}

	function handleSend() {
		if (!inputText.trim() || $chat.isStreaming) return;
		chat.send(inputText);
		inputText = '';
	}

	function handleKeydown(event: KeyboardEvent) {
		if (event.key !== 'Enter') return;
		if (event.shiftKey) return;
		event.preventDefault();
		handleSend();
	}
</script>

<div class="app">
	<header>
		<div class="status" class:connected={$chat.isConnected}></div>
		<b>agents-rs</b>
	</header>

	<main>
		<div class="messages" bind:this={messagesContainer}>
			{#each $chat.messages as message}
				<div
					class="message"
					class:user={message.user === 'User'}
					class:bot={message.user === 'Bot'}
					class:streaming={message.streaming}
				>
					{message.msg}
				</div>
			{/each}
		</div>

		<div class="input-area">
			<textarea
				bind:value={inputText}
				onkeydown={handleKeydown}
				placeholder="Type a message..."
				disabled={!$chat.isConnected}
				rows="1"
			></textarea>
			<button
				onclick={handleSend}
				disabled={!$chat.isConnected || $chat.isStreaming || !inputText.trim()}
			>
				Send
			</button>
		</div>
	</main>
</div>
