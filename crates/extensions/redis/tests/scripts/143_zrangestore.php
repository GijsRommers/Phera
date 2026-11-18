<?php
require __DIR__."/../support.php";
$r->zAdd($p."a",1,"a",2,"b");
check(fn()=>$r->zrangestore($p."b",$p."a","0","-1"));
check(fn()=>$r->zRange($p."b",0,-1,true));
finish();
