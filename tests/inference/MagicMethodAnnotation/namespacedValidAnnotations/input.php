<?php
namespace Foo;

class ParentClass {
    public function __call(string $name, array $args) {}
}

/**
 * @method setBool(string $foo, string|bool $bar)  :   bool
 */
class Child extends ParentClass {}

$child = new Child();

$c = $child->setBool("hello", true);
$c = $child->setBool("hello", "true");
