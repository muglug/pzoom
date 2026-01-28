<?php
class ParentClass {
    public function __call(string $name, array $args) {}
}

/**
 * @method void setArray(array $arr = array(1, 2, 3)) with some more text
 */
class Child extends ParentClass {}

$child = new Child();

$child->setArray(["boo"]);
$child->setArray(["boo"]);
