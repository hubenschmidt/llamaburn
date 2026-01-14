export type WsMetadata = {
	input_tokens: number;
	output_tokens: number;
	elapsed_ms: number;
	load_duration_ms?: number;
	prompt_eval_ms?: number;
	eval_ms?: number;
	tokens_per_sec?: number;
};

export type ModelConfig = {
	id: string;
	name: string;
	model: string;
	api_base: string | null;
};

export type ChatMsg = {
	user: 'User' | 'Bot';
	msg: string;
	streaming?: boolean;
	metadata?: WsMetadata;
};

export type WsPayload = {
	uuid?: string;
	message?: string;
	model_id?: string;
	init?: boolean;
	verbose?: boolean;
};

export type WsResponse = {
	on_chat_model_stream?: string;
	on_chat_model_end?: boolean;
	metadata?: WsMetadata;
	models?: ModelConfig[];
};
