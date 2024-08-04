BEGIN;

CREATE TABLE public.lunch_polls (
    id bigint NOT NULL,
    tg_chat_id bigint NOT NULL,
    tg_poll_id character varying NOT NULL,
    tg_poll_msg_id integer NOT NULL,
    yes_voters jsonb NOT NULL DEFAULT '[]'::jsonb,
    created_at timestamp without time zone NOT NULL,
    updated_at timestamp without time zone NOT NULL
);

CREATE SEQUENCE public.lunch_polls_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;

ALTER SEQUENCE public.lunch_polls_id_seq OWNED BY public.lunch_polls.id;

ALTER TABLE ONLY public.lunch_polls ALTER COLUMN id SET DEFAULT nextval('public.lunch_polls_id_seq'::regclass);

CREATE UNIQUE INDEX index_lunch_polls_on_tg_chat_id ON public.lunch_polls USING btree (tg_chat_id);

COMMIT;
