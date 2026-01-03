import { writable } from 'svelte/store';
import type { ChatMsg, WsPayload, WsResponse } from '$lib/types';

function createChatStore() {
	const messages = writable<ChatMsg[]>([
		{ user: 'Bot', msg: 'Welcome! How can I help you today?' }
	]);
	const isConnected = writable(false);
	const isStreaming = writable(false);

	let ws: WebSocket | null = null;
	const uuid = crypto.randomUUID();

	function connect(url: string) {
		ws = new WebSocket(url);

		ws.onopen = () => {
			isConnected.set(true);
			const payload: WsPayload = { uuid, init: true };
			ws?.send(JSON.stringify(payload));
		};

		ws.onclose = () => {
			isConnected.set(false);
			isStreaming.set(false);
		};

		ws.onerror = () => {
			isConnected.set(false);
		};

		ws.onmessage = (event) => {
			const data: WsResponse = JSON.parse(event.data);

			if (data.on_chat_model_stream !== undefined) {
				handleStreamChunk(data.on_chat_model_stream);
				return;
			}

			if (data.on_chat_model_end) {
				handleStreamEnd();
			}
		};
	}

	function handleStreamChunk(chunk: string) {
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

	function handleStreamEnd() {
		isStreaming.set(false);
		messages.update((msgs) => {
			const last = msgs[msgs.length - 1];
			if (last?.streaming) {
				return [...msgs.slice(0, -1), { ...last, streaming: false }];
			}
			return msgs;
		});
	}

	function send(text: string) {
		if (!ws || !text.trim()) return;

		messages.update((msgs) => [...msgs, { user: 'User', msg: text }]);

		const payload: WsPayload = { uuid, message: text };
		ws.send(JSON.stringify(payload));
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
		connect,
		send,
		reset,
		disconnect
	};
}

export const chat = createChatStore();
