<?php
require __DIR__ . '/../support.php';
$username=$p.'user';
$r->rawCommand('ACL','SETUSER',$username,'on','>secret','~'.$p.'*','+@all');
try {
    $c=new Redis();$c->connect('127.0.0.1',16399);$c->select(14);
    foreach ([['pass'=>'secret','user'=>$username],[$username,'secret'],[],['password'=>'x'],
        [false,'incorrect'],['incorrect'],false,null,42,true] as $credentials) {
        check(fn()=>$c->auth($credentials));check(fn()=>$c->getAuth());
    }
    check(fn()=>$c->auth(['user'=>$username,'pass'=>'secret']));
    foreach (['multi','pipeline'] as $mode) {
        $c->$mode();check(fn()=>$c->auth(['pass'=>'secret','user'=>$username]));
        check(fn()=>$c->getAuth());check(fn()=>$c->exec());
    }
    $c->close();
    $c=new Redis(['host'=>'127.0.0.1','port'=>16399,'database'=>14,
        'auth'=>['pass'=>'secret','user'=>$username]]);
    check(fn()=>$c->ping());check(fn()=>$c->getAuth());$c->close();
} finally {$r->rawCommand('ACL','DELUSER',$username);}
finish();
