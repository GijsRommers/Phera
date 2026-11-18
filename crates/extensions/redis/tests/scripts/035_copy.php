<?php
require __DIR__."/../support.php";
check(fn()=>$r->copy($p."a",$p."b",["REPLACE"]));$r->set($p."a","v\0");check(fn()=>$r->copy($p."a",$p."b",["REPLACE"]));
check(fn()=>$r->copy($p."a",$p."b",["REPLACE"]));check(fn()=>$r->get($p."b"));
finish();
