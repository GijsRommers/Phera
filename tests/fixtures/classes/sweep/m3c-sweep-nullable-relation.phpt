--TEST--
Application class sweep: nullable-relation
--FILE--
<?php
class Account {
public ?Account $manager=null;
function __construct(
public string $name)
{
}
function path(
)
: string {
if(
$this->manager===null)
{
return $this->name;
}
return $this->manager->path(
)
.'/'.$this->name;
}
}
$root=new Account(
'root')
;
$team=new Account('team');
$user=new Account('user');
$team->manager=$root;
$user->manager=$team;
echo $user->path(),
"\n",
$team->path(),
"\n";

--EXPECT--
root/team/user
root/team
