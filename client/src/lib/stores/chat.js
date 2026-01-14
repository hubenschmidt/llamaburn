import { writable, get } from 'svelte/store';
import { devMode } from './settings.js';

function createChatStore() {
	const messages = writable([
		{ user: 'Bot', msg: 'Welcome! How can I help you today?' }
	]);
	const isConnected = writable(false);
	const isStreaming = writable(false);
	const isThinking = writable(false);
	const models = writable([]);
	const selectedModel = writable('');
	const modelStatus = writable('');

	let ws = null;
	const uuid = crypto.randomUUID();

	function connect(url) {
		ws = new WebSocket(url);

		ws.onopen = () => {
			isConnected.set(true);
			const payload = { uuid, init: true };
			ws?.send(JSON.stringify(payload));
		};

		ws.onclose = () => {
			isConnected.set(false);
			isStreaming.set(false);
			isThinking.set(false);
		};

		ws.onerror = () => {
			isConnected.set(false);
		};

		ws.onmessage = (event) => {
			const data = JSON.parse(event.data);

			if (data.models) {
				models.set(data.models);
				if (data.models.length > 0 && !get(selectedModel)) {
					selectedModel.set(data.models[0].id);
				}
				return;
			}

			if (data.model_status !== undefined) {
				modelStatus.set(data.model_status);
				return;
			}

			if (data.on_chat_model_stream !== undefined) {
				handleStreamChunk(data.on_chat_model_stream);
				return;
			}

			if (data.on_chat_model_end) {
				handleStreamEnd(data.metadata);
			}
		};
	}

	function handleStreamChunk(chunk) {
		isThinking.set(false);
		messages.update((msgs) => {
			const last = msgs[msgs.length - 1];

			if (last?.user === 'Bot' && last.streaming) {
				return [
					...msgs.slice(0, -1),
					{ user: 'Bot', msg: last.msg + chunk, streaming: true }
				];
			}

			isStreaming.set(true);
			return [...msgs, { user: 'Bot', msg: chunk, streaming: true }];
		});
	}

	function handleStreamEnd(metadata) {
		isStreaming.set(false);
		isThinking.set(false);
		messages.update((msgs) => {
			const last = msgs[msgs.length - 1];
			if (last?.streaming) {
				return [...msgs.slice(0, -1), { ...last, streaming: false, metadata }];
			}
			return msgs;
		});
	}

	function send(text) {
		if (!ws || !text.trim()) return;

		messages.update((msgs) => [...msgs, { user: 'User', msg: text }]);
		isThinking.set(true);

		const payload = {
			uuid,
			message: text,
			model_id: get(selectedModel),
			verbose: get(devMode)
		};
		ws.send(JSON.stringify(payload));
	}

	function wake(modelId, previousModelId) {
		if (!ws) return;
		const payload = {
			uuid,
			wake_model_id: modelId,
			unload_model_id: previousModelId
		};
		ws.send(JSON.stringify(payload));
	}

	function unload(modelId) {
		if (!ws) return;
		const payload = {
			uuid,
			unload_model_id: modelId
		};
		ws.send(JSON.stringify(payload));
	}

	function isLocalModel(modelId) {
		const model = get(models).find((m) => m.id === modelId);
		return model?.api_base !== null && model?.api_base !== undefined;
	}

	function reset() {
		messages.set([{ user: 'Bot', msg: 'Welcome! How can I help you today?' }]);
	}

	function disconnect() {
		ws?.close();
		ws = null;
	}

	return {
		messages,
		isConnected,
		isStreaming,
		isThinking,
		models,
		selectedModel,
		modelStatus,
		connect,
		send,
		wake,
		unload,
		isLocalModel,
		reset,
		disconnect
	};
}

export const chat = createChatStore();
