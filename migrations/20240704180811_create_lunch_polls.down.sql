BEGIN;

DROP INDEX index_lunch_polls_on_tg_chat_id;
DROP TABLE public.lunch_polls; -- drops primary key sequence along with it

COMMIT;
