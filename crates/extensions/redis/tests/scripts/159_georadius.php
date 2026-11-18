<?php
require __DIR__."/../support.php";
$r->geoAdd($p."g",13.361389,38.115556,"Palermo",15.087269,37.502669,"Catania");
check(fn()=>$r->geoRadius($p."g",13.361389,38.115556,1,"km"));
finish();
