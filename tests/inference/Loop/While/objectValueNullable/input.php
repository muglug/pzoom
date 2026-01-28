<?php
class A {
    /** @var ?A */
    public $parent;

    public function __construct() {
        $this->parent = rand(0, 1) ? new A() : null;
    }
}

function makeA(): A {
    return new A();
}

$a = makeA();

while ($a) {
    $a = $a->parent;
}
