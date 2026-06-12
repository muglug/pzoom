<?php
class ParentClass {
    public function __call(string $name, array $args) {}
}

/**
 * @method string getString()
 */
class Child extends ParentClass {}

$child = new Child();
/** @psalm-suppress UndefinedMagicMethod */
$child->foo();
