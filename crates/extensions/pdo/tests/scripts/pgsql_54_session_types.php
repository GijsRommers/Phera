<?php
$dsn='pgsql:host=127.0.0.1;port=54329;dbname=phera_test';
$p=new PDO($dsn,'phera','phera');
foreach(['UTC','Europe/Brussels','America/New_York','Asia/Kathmandu'] as $zone) {
$p->exec("SET TIME ZONE '$zone'");
var_dump($p->query("SELECT '2026-11-17 12:34:56.123456+00'::timestamptz AS tz,
'12:34:56.1200'::time AS tm, '12:34:56.12+05:45'::timetz AS tmz,
'1 year 2 months 3 days 04:05:06.12'::interval AS iv,
'12345678-1234-5678-9abc-123456789abc'::uuid AS u, '192.168.1.4/24'::inet AS ip,
'2001:db8::1'::inet AS ip6")->fetch(PDO::FETCH_ASSOC));
}
$p->exec("SET intervalstyle = 'iso_8601'");
var_dump($p->query("SELECT '2 months -4 days 05:06:07'::interval")->fetchColumn());
