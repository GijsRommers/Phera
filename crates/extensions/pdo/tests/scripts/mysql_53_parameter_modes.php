<?php
$dsn='mysql:host=127.0.0.1;port=33069;dbname=phera_test';
$p=new PDO($dsn,'phera','phera');
$p->setAttribute(PDO::ATTR_EMULATE_PREPARES,false);$s=$p->prepare("SELECT ? AS a, ? AS b");
try {$s->execute([]);} catch(Throwable $e) {echo $s->errorCode(),"\n";}
$p->setAttribute(PDO::ATTR_EMULATE_PREPARES,false);$s=$p->prepare("SELECT ? AS a, ? AS b");
try {$s->execute([1]);} catch(Throwable $e) {echo $s->errorCode(),"\n";}
$p->setAttribute(PDO::ATTR_EMULATE_PREPARES,false);$s=$p->prepare("SELECT ? AS a, ? AS b");
try {$s->execute([1,2,3]);} catch(Throwable $e) {echo $s->errorCode(),"\n";}
$p->setAttribute(PDO::ATTR_EMULATE_PREPARES,false);$s=$p->prepare("SELECT ? AS a, ? AS b");
try {$s->execute(['a'=>1,'b'=>2,'c'=>3]);} catch(Throwable $e) {echo $s->errorCode(),"\n";}
$p->setAttribute(PDO::ATTR_EMULATE_PREPARES,false);$s=$p->prepare("SELECT :a AS a, :b AS b");
try {$s->execute([]);} catch(Throwable $e) {echo $s->errorCode(),"\n";}
$p->setAttribute(PDO::ATTR_EMULATE_PREPARES,false);$s=$p->prepare("SELECT :a AS a, :b AS b");
try {$s->execute([1]);} catch(Throwable $e) {echo $s->errorCode(),"\n";}
$p->setAttribute(PDO::ATTR_EMULATE_PREPARES,false);$s=$p->prepare("SELECT :a AS a, :b AS b");
try {$s->execute([1,2,3]);} catch(Throwable $e) {echo $s->errorCode(),"\n";}
$p->setAttribute(PDO::ATTR_EMULATE_PREPARES,false);$s=$p->prepare("SELECT :a AS a, :b AS b");
try {$s->execute(['a'=>1,'b'=>2,'c'=>3]);} catch(Throwable $e) {echo $s->errorCode(),"\n";}
$p->setAttribute(PDO::ATTR_EMULATE_PREPARES,true);$s=$p->prepare("SELECT ? AS a, ? AS b");
try {$s->execute([]);} catch(Throwable $e) {echo $s->errorCode(),"\n";}
$p->setAttribute(PDO::ATTR_EMULATE_PREPARES,true);$s=$p->prepare("SELECT ? AS a, ? AS b");
try {$s->execute([1]);} catch(Throwable $e) {echo $s->errorCode(),"\n";}
$p->setAttribute(PDO::ATTR_EMULATE_PREPARES,true);$s=$p->prepare("SELECT ? AS a, ? AS b");
try {$s->execute([1,2,3]);} catch(Throwable $e) {echo $s->errorCode(),"\n";}
$p->setAttribute(PDO::ATTR_EMULATE_PREPARES,true);$s=$p->prepare("SELECT ? AS a, ? AS b");
try {$s->execute(['a'=>1,'b'=>2,'c'=>3]);} catch(Throwable $e) {echo $s->errorCode(),"\n";}
$p->setAttribute(PDO::ATTR_EMULATE_PREPARES,true);$s=$p->prepare("SELECT :a AS a, :b AS b");
try {$s->execute([]);} catch(Throwable $e) {echo $s->errorCode(),"\n";}
$p->setAttribute(PDO::ATTR_EMULATE_PREPARES,true);$s=$p->prepare("SELECT :a AS a, :b AS b");
try {$s->execute([1]);} catch(Throwable $e) {echo $s->errorCode(),"\n";}
$p->setAttribute(PDO::ATTR_EMULATE_PREPARES,true);$s=$p->prepare("SELECT :a AS a, :b AS b");
try {$s->execute([1,2,3]);} catch(Throwable $e) {echo $s->errorCode(),"\n";}
$p->setAttribute(PDO::ATTR_EMULATE_PREPARES,true);$s=$p->prepare("SELECT :a AS a, :b AS b");
try {$s->execute(['a'=>1,'b'=>2,'c'=>3]);} catch(Throwable $e) {echo $s->errorCode(),"\n";}
