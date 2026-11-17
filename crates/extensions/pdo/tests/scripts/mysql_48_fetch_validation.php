<?php
$dsn='mysql:host=127.0.0.1;port=33069;dbname=phera_test';
$p=new PDO($dsn,'phera','phera');
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($p->query("SELECT 10 AS a,20 AS b,30 AS c", PDO::FETCH_ASSOC, 1)->fetchAll());} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($p->query("SELECT 10 AS a,20 AS b,30 AS c", PDO::FETCH_COLUMN)->fetchAll());} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($p->query("SELECT 10 AS a,20 AS b,30 AS c", PDO::FETCH_COLUMN, -1)->fetchAll());} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($p->query("SELECT 10 AS a,20 AS b,30 AS c", PDO::FETCH_COLUMN, 9)->fetchAll());} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($p->query("SELECT 10 AS a,20 AS b,30 AS c", PDO::FETCH_COLUMN, "x")->fetchAll());} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($p->query("SELECT 10 AS a,20 AS b,30 AS c", PDO::FETCH_COLUMN, 0, 1)->fetchAll());} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($p->query("SELECT 10 AS a,20 AS b,30 AS c", 15)->fetchAll());} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($p->query("SELECT 10 AS a,20 AS b,30 AS c", 999)->fetchAll());} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($p->query("SELECT 10 AS a,20 AS b,30 AS c", PDO::FETCH_GROUP | PDO::FETCH_COLUMN, 1)->fetchAll());} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($p->query("SELECT 10 AS a,20 AS b,30 AS c", PDO::FETCH_UNIQUE | PDO::FETCH_COLUMN, 1)->fetchAll());} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($s->setFetchMode(PDO::FETCH_ASSOC, 1));} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($s->setFetchMode(PDO::FETCH_COLUMN));} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($s->setFetchMode(PDO::FETCH_COLUMN, -1));} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($s->setFetchMode(PDO::FETCH_COLUMN, 9));} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($s->setFetchMode(PDO::FETCH_COLUMN, "x"));} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($s->setFetchMode(PDO::FETCH_COLUMN, 0, 1));} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($s->setFetchMode(15));} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($s->setFetchMode(999));} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($s->setFetchMode(PDO::FETCH_GROUP | PDO::FETCH_COLUMN, 1));} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($s->setFetchMode(PDO::FETCH_UNIQUE | PDO::FETCH_COLUMN, 1));} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($s->fetchAll(PDO::FETCH_ASSOC, 1));} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($s->fetchAll(PDO::FETCH_COLUMN));} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($s->fetchAll(PDO::FETCH_COLUMN, -1));} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($s->fetchAll(PDO::FETCH_COLUMN, 9));} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($s->fetchAll(PDO::FETCH_COLUMN, "x"));} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($s->fetchAll(PDO::FETCH_COLUMN, 0, 1));} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($s->fetchAll(15));} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($s->fetchAll(999));} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($s->fetchAll(PDO::FETCH_GROUP | PDO::FETCH_COLUMN, 1));} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->query("SELECT 10 AS a,20 AS b,30 AS c");
try {var_dump($s->fetchAll(PDO::FETCH_UNIQUE | PDO::FETCH_COLUMN, 1));} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
