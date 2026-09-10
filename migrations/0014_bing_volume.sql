-- Monthly searches on Bing, next to Google's.
--
-- Bing publishes a real count through Microsoft Advertising, which DataForSEO
-- resells (keywords_data/bing/search_volume). It exists for six countries
-- only: US, GB, CA, AU, DE, FR, in en/de/fr. Checked against the API's own
-- location list: no Polish location at all, so for PL runs this stays null
-- and the page says why. Null is "not available", never "zero".

alter table suggestions add column if not exists bing_volume bigint;
