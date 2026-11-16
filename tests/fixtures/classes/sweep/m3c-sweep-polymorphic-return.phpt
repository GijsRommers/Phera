--TEST--
Application class sweep: polymorphic-return
--FILE--
<?php
interface Shape {
function area(
)
: int;
}
class Square implements Shape {
function __construct(public int $side){
}
function area(): int {
return $this->side*$this->side;
}
}
class Rectangle implements Shape {
function __construct(public int $width,
public int $height){
}
function area(): int {
return $this->width*$this->height;
}
}
function area(Shape $shape): int {
return $shape->area();
}
$shapes=[new Square(3),
new Rectangle(2,
4)];
foreach($shapes as $shape){
echo area($shape),
"\n";
}

--EXPECT--
9
8
