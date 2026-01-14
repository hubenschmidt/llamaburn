<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { chat } from '$lib/stores/chat';
	import Header from '$lib/components/Header.svelte';
	import ChatMessage from '$lib/components/ChatMessage.svelte';
	import ChatInput from '$lib/components/ChatInput.svelte';

	const { messages, isConnected, isStreaming, isThinking, models, selectedModel } = chat;
	const WS_URL = 'ws://localhost:8000/ws';

	let inputText = '';
	let messagesContainer: HTMLDivElement;
	let prevModel = '';

	onMount(() => {
		chat.connect(WS_URL);
		return () => chat.disconnect();
	});

	$: if ($selectedModel && $selectedModel !== prevModel) {
		if (prevModel && chat.isLocalModel($selectedModel)) {
			chat.wake($selectedModel);
		}
		prevModel = $selectedModel;
	}

	async function scrollToBottom() {
		await tick();
		if (messagesContainer) {
			messagesContainer.scrollTop = messagesContainer.scrollHeight;
		}
	}

	$: if ($messages || $isThinking) {
		scrollToBottom();
	}

	function handleSend() {
		if (!inputText.trim() || $isStreaming) return;
		chat.send(inputText);
		inputText = '';
	}
</script>

<div class="app">
	<Header
		isConnected={$isConnected}
		models={$models}
		bind:selectedModel={$selectedModel}
	/>

	<main>
		<div class="messages" bind:this={messagesContainer}>
			{#each $messages as message}
				<ChatMessage
					user={message.user}
					msg={message.msg}
					streaming={message.streaming}
					metadata={message.metadata}
				/>
			{/each}
			{#if $isThinking}
				<div class="message bot thinking">
					<span class="thinking-dots">
						<span></span>
						<span></span>
						<span></span>
					</span>
				</div>
			{/if}
		</div>

		<ChatInput
			bind:value={inputText}
			disabled={!$isConnected}
			sendDisabled={!$isConnected || $isStreaming || !inputText.trim()}
			onSend={handleSend}
		/>
	</main>
</div>
