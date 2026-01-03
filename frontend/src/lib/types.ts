export type ChatMsg = {
	user: 'User' | 'Bot';
	msg: string;
	streaming?: boolean;
};

export type WsPayload = {
	uuid?: string;
	message?: string;
	init?: boolean;
};

export type WsResponse = {
	on_chat_model_stream?: string;
	on_chat_model_end?: boolean;
};
