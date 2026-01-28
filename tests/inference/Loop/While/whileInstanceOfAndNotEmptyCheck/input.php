<?php
class A {
    /** @var null|A */
    public $parent;
}

class B extends A {}

$a = (new A())->parent;

$foo = rand(0, 1) ? "hello" : null;

if (!$foo) {
    while ($a instanceof B && !$foo) {
        $a = $a->parent;
        $foo = rand(0, 1) ? "hello" : null;
    }
}
