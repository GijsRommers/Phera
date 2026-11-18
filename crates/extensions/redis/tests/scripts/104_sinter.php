<?php
require __DIR__."/../support.php";
$r->sAdd($p."a","a");$r->sAdd($p."b","a");
check(fn()=>$r->sInter($p."a",$p."b"));$r->del($p."b");check(fn()=>$r->sInter($p."a",$p."b"));
finish();
