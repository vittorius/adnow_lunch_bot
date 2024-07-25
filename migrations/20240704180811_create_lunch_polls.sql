-- Add migration script here
CREATE TABLE public.lunch_polls (
    id bigint NOT NULL,
    chat_id bigint NOT NULL,
    poll_id character varying NOT NULL,
    yes_voters jsonb NOT NULL DEFAULT '[]'::jsonb,
    created_at timestamp without time zone NOT NULL,
    updated_at timestamp without time zone NOT NULL
);

CREATE UNIQUE INDEX index_lunch_polls_on_chat_id ON public.lunch_polls USING btree (chat_id);
