<?php
require __DIR__."/../support.php";
$r->zAdd($p."a",1,"a");$r->zAdd($p."b",2,"a");
check(fn()=>$r->zinterstore($p."dst",[$p."a",$p."b"]));$r->del($p."b");check(fn()=>$r->zinterstore($p."dst",[$p."a",$p."b"]));
finish();
