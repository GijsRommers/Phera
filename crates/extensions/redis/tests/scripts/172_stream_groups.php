<?php
require __DIR__ . '/../support.php';
$r->xAdd($p.'x', '1-0', ['a'=>'v']);
$r->xAdd($p.'x', '2-0', ['b'=>'w']);
check(fn()=>$r->xGroup('CREATE',$p.'x','g','0'));
check(fn()=>$r->xReadGroup('g','c',[$p.'x'=>'>'],10,0));
check(fn()=>$r->xPending($p.'x','g'));
check(fn()=>$r->xClaim($p.'x','g','c2',0,['1-0'],[]));
check(fn()=>$r->xAutoClaim($p.'x','g','c3',0,'0-0',10));
check(fn()=>$r->xAck($p.'x','g',['1-0','2-0']));
check(fn()=>$r->xInfo('STREAM',$p.'x'));
check(fn()=>$r->xGroup('DESTROY',$p.'x','g'));
finish();
