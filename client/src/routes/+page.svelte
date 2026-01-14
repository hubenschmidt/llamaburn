<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { chat } from '$lib/stores/chat';
	import Header from '$lib/components/Header.svelte';
	import ChatMessage from '$lib/components/ChatMessage.svelte';
	import ChatInput from '$lib/components/ChatInput.svelte';

	const { messages, isConnected, isStreaming, isThinking, models, selectedModel, modelStatus } = chat;
	const WS_URL = 'ws://localhost:8000/ws';

	let inputText = '';
	let messagesContainer: HTMLDivElement;
	let prevModel = '';

	onMount(() => {
		chat.connect(WS_URL);
		return () => chat.disconnect();
	});

	$: if ($selectedModel !== prevModel) {
		handleModelChange(prevModel, $selectedModel);
		prevModel = $selectedModel;
	}

	function handleModelChange(prev: string, next: string) {
		const prevIsLocal = prev && chat.isLocalModel(prev);
		const nextIsLocal = next && chat.isLocalModel(next);

		// Unload GPU: switching to "none"
		if (next === 'none' && prevIsLocal) {
			chat.unload(prev);
			return;
		}

		// Switching to a local model: wake it (and unload previous if also local)
		if (nextIsLocal) {
			const prevToUnload = prevIsLocal ? prev : undefined;
			chat.wake(next, prevToUnload);
			return;
		}

		// Switching from local to cloud: unload the local model
		if (prevIsLocal && !nextIsLocal) {
			chat.unload(prev);
		}
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
		modelStatus={$modelStatus}
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
			disabled={!$isConnected || $selectedModel === 'none'}
			sendDisabled={!$isConnected || $isStreaming || !inputText.trim() || $selectedModel === 'none'}
			onSend={handleSend}
		/>
	</main>
</div>
