<?php
$dsn='pgsql:host=127.0.0.1;port=54329;dbname=phera_test';
$p=new PDO($dsn,'phera','phera');
$p->exec("CREATE TEMPORARY TABLE metadata_probe(id INT PRIMARY KEY, txt VARCHAR(23) NOT NULL, n DECIMAL(9,2))");
$s=$p->query("SELECT id, txt, n, 1 AS x, NULL AS nul FROM metadata_probe");
for($i=0;$i<6;$i++) {$m=$s->getColumnMeta($i);
if(is_array($m) && isset($m['pgsql:table_oid'])) {$m['pgsql:table_oid']=($m['pgsql:table_oid']>0);}
var_dump($m);}
try {$s->getColumnMeta(-1);} catch(Throwable $e) {echo $e->getMessage(),"\n";}
