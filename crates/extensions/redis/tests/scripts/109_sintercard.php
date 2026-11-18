<?php
require __DIR__."/../support.php";
$r->sAdd($p."a","a","b");$r->sAdd($p."b","a");
foreach ([-1,0,1,2] as $limit) {check(fn()=>$r->sInterCard([$p."a",$p."b"],$limit));}
finish();
