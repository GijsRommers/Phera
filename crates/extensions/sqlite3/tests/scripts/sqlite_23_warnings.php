<?php
$d=new SQLite3(':memory:');
set_error_handler(function($n,$m){
    echo "$n $m\n";
    return true;
}
);
var_dump($d->enableExceptions(false));
foreach(['exec','query','prepare','querySingle'] as $m)var_dump($d
    ->$m('SELECT missing'));
