<?php
/**
 * @psalm-consistent-constructor
 */
class Foo {
    public function bar() : void{}
}

/**
 * @return string|null
 */
function getFooClass() {
    return mt_rand(0, 1) === 1 ? Foo::class : null;
}

$foo_class = getFooClass();

if (is_string($foo_class) && is_a($foo_class, Foo::class, true)) {
    $foo = new $foo_class();
    $foo->bar();
}
