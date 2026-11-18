<?php
$dsn='mysql:host=127.0.0.1;port=33069;dbname=phera_test';
$p=new PDO($dsn,'phera','phera');
$s=$p->prepare("SELECT ? AS a");$v=1;
try {var_dump($s->bindValue(0,$v));$s->execute();} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->prepare("SELECT ? AS a");$v=1;
try {var_dump($s->bindValue(-1,$v));$s->execute();} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->prepare("SELECT ? AS a");$v=1;
try {var_dump($s->bindValue("",$v));$s->execute();} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->prepare("SELECT ? AS a");$v=1;
try {var_dump($s->bindValue("missing",$v));$s->execute();} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->prepare("SELECT ? AS a");$v=1;
try {var_dump($s->bindValue(3,$v));$s->execute();} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->prepare("SELECT ? AS a");$v=1;
try {var_dump($s->bindParam(0,$v));$s->execute();} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->prepare("SELECT ? AS a");$v=1;
try {var_dump($s->bindParam(-1,$v));$s->execute();} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->prepare("SELECT ? AS a");$v=1;
try {var_dump($s->bindParam("",$v));$s->execute();} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->prepare("SELECT ? AS a");$v=1;
try {var_dump($s->bindParam("missing",$v));$s->execute();} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
$s=$p->prepare("SELECT ? AS a");$v=1;
try {var_dump($s->bindParam(3,$v));$s->execute();} catch(Throwable $e) {echo get_class($e),": ",$e->getMessage(),"\n";}
foreach([-1,3] as $index) {$s=$p->query("SELECT 1 AS a");try {$s->fetchColumn($index);} catch(Throwable $e) {echo $e->getMessage(),"\n";}var_dump($s->fetchColumn());}
