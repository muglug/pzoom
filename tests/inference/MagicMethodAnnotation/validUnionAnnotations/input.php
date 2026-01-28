<?php
class ParentClass {
    public function __call(string $name, array $args) {}
}

/**
 * @method setBool(string $foo, string|bool $bar)  :   bool dsa sada
 * @method void setAnotherArray(int[]|string[] $arr = [], int $foo = 5) with some more text
 */
class Child extends ParentClass {}

$child = new Child();

$b = $child->setBool("hello", true);
$c = $child->setBool("hello", "true");
$child->setAnotherArray(["boo"]);
