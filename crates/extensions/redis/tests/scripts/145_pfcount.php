<?php
require __DIR__."/../support.php";
check(fn()=>$r->pfCount([$p."a",$p."b"]));$r->pfAdd($p."a",["a","b"]);
check(fn()=>$r->pfCount([$p."a",$p."b"]));check(fn()=>$r->pfCount($p."b"));
finish();
