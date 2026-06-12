<?php
/**
 * @method boolean(int $foo) : bool
 * @method integer(int $foo) : bool
 */
class Child {
    public function __call(string $name, array $args) {}
}

$child = new Child();

$child->boolean(5);
$child->integer(5);
