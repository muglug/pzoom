<?php
class ParentClass {
    public function __call(string $name, array $args) {}
}

/**
 * @template T
 * @method void configure(string $string, array &$arr)
 */
class Child extends ParentClass
{
    /** @psalm-param T $t */
    public function getChild($t): void {}
}
$child = new Child();

$array = [];
$child->configure("foo", $array);
