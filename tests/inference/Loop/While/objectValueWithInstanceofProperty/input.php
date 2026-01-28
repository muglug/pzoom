<?php
class B {}
class A {
    /** @var A|B */
    public $parent;

    public function __construct() {
        $this->parent = rand(0, 1) ? new A() : new B();
    }
}

function makeA(): A {
    return new A();
}

$a = makeA();

while ($a->parent instanceof A) {
    $a = $a->parent;
}

$b = $a->parent;
