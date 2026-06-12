<?php
class ParentClass {
    public function __call(string $name, array $args) {}
}

/**
 * @method int getString() dsa sada
 * @method  void setInteger(string $integer) dsa sada
 * @psalm-method string getString() dsa sada
 * @psalm-method  void setInteger(int $integer) dsa sada
 */
class Child extends ParentClass {}

$child = new Child();

$a = $child->getString();
$child->setInteger(4);
