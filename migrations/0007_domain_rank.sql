-- Domain authority of each competing page, 0-1000 from DataForSEO backlinks.
--
-- Volume and CPC say how wanted a topic is; this says how hard the current
-- top 10 is to displace. Measured: "czy kawa jest zdrowa" has a median of 317
-- across its competitors, "kredyt hipoteczny" 548, which matches what anyone
-- who has tried to rank in finance would expect.

alter table brief_competitors add column if not exists domain_rank int;
