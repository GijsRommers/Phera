--TEST--
Application class sweep: abstract-template
--FILE--
<?php
abstract class Exporter {
function export(
$rows)
{
$result=[];
foreach(
$rows as $row)
{
$result[]=$this->format(
$row);
}
 return $result;
}
 abstract protected function format($row);
}
 class CsvExporter extends Exporter {
protected function format($row){
return $row['id'].',
'.$row['name'];
}
}
 $exporter=new CsvExporter;
 $rows=[['id'=>1,
'name'=>'a'],
['id'=>2,
'name'=>'b']];
 foreach($exporter->export($rows) as $line){
echo $line,
"\n";
}

--EXPECT--
1,
a
2,
b
