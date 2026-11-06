-- Synthetic acceptance data. Apply only to a fresh isolated database after migrations.
insert into searches(id,keyword,language,country,status,provider,source,suggestion_count)
values ('11111111-1111-4111-8111-111111111111','ekspres do kawy','pl','pl','done','dataforseo','google',8);
insert into suggestions(search_id,text,category,modifier,search_volume,ai_volume)
select '11111111-1111-4111-8111-111111111111'::uuid, text, category, modifier, volume, asked from (values
('jak wybrać ekspres do kawy','questions','jak',1000,100),
('Jak wybrać ekspres do kawy?','questions','jak',990,100),
('czy ekspres do kawy jest zdrowy','questions','czy',600,80),
('ekspres do kawy delonghi vs philips','comparisons','vs',300,30),
('alternatywa dla ekspresu do kawy','related','',200,20),
('najlepszy ekspres do kawy','alphabetical','n',800,40),
('ekspres do kawy','related','',3000,60),
('jak czyścić ekspres do kawy','questions','jak',500,70)
) as data(text,category,modifier,volume,asked);
insert into ai_answer_runs(id,search_id,topic,language,country,domain,model)
values ('22222222-2222-4222-8222-222222222222','11111111-1111-4111-8111-111111111111','ekspres do kawy','pl','pl','example.com','sonar');
insert into ai_answers(run_id,question,answer,domains,urls)
select '22222222-2222-4222-8222-222222222222'::uuid, question, answer, domains::jsonb, urls::jsonb from (values
('jak wybrać ekspres do kawy','Przykładowa odpowiedź. Zobacz example.com. To dane testowe.','["example.com"]','["https://example.com/guide"]'),
('ekspres do kawy delonghi vs philips','Porównanie z zapisanym źródłem.','["blog.example.com"]','["https://blog.example.com/comparison"]'),
('czy ekspres do kawy jest zdrowy','Wspomnienie example.com bez źródła.','[]','[]'),
('jak czyścić ekspres do kawy','Nie pasuje: notexample.com oraz example.com.evil.test.','["notexample.com"]','["https://example.com.evil.test/guide"]')
) as data(question,answer,domains,urls);
insert into briefs(id,topic,language,country,status,search_id,ai_overview,ai_sources,finished_at,ai_answers)
values ('33333333-3333-4333-8333-333333333333','jak wybrać ekspres do kawy','pl','pl','done','11111111-1111-4111-8111-111111111111','Zapisany testowy AI Overview.','["example.com"]',now(),
'[{"question":"jaki ekspres do kawy kupić","answer":"Zapisana odpowiedź z briefu","domains":["example.com"],"urls":["https://example.com/buy"],"cited":false,"cited_rank":null}]');
