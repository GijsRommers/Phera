--TEST--
Application class sweep: repository loops filter and sort objects
--FILE--
<?php
class Record
{
    function __construct(public int $id, public int $score) {}
}
class Repository
{
    public array $records=[];
    function add(Record $record): void
    {
        $this->records[]=$record;
    }
    function best(): Record
    {
        $best=$this->records[0];
        foreach($this->records as $record) {
            if($record->score>$best->score) $best=$record;
        }
        return $best;
    }
    function sortedAtLeast(int $minimum): array
    {
        $result=[];
        foreach($this->records as $record) {
            if($record->score >= $minimum) $result[]=$record;
        }
        for($left=0;$left<count($result);$left++) {
            for($right=$left+1;$right<count($result);$right++) {
                if($result[$right]->score>$result[$left]->score) {
                    $temporary=$result[$left];
                    $result[$left]=$result[$right];
                    $result[$right]=$temporary;
                }
            }
        }
        return $result;
    }
}
$repository=new Repository;
$repository->add(new Record(1,8));
$repository->add(new Record(2,12));
$repository->add(new Record(3,7));
echo $repository->best()->id;
echo "\n";
foreach($repository->sortedAtLeast(8) as $record) {
    echo $record->id,'=',$record->score,"\n";
}
foreach($repository->records as $record) {
    echo $record->id;
}
echo "\n";
--EXPECT--
2
2=12
1=8
123
