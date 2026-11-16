--TEST--
Application class sweep: interface-constant
--FILE--
<?php
interface Status {
const ACTIVE=1;
const INACTIVE=0;
function status(
)
: int;
}
 class User implements Status {
function status(
)
: int {
return self::ACTIVE;
}
}
 class Guest implements Status {
function status(
): int {
return Status::INACTIVE;
}
}
$items=[new User,
new Guest];
foreach($items as $item){
echo get_class($item),
':',
$item->status(),
'/',
Status::ACTIVE,
"\n";
}

--EXPECT--
User:1/1
Guest:0/1
