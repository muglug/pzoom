<?php
class A {
    /** @var int */
    public $foo = 0;
}
class B {
    /** @var string */
    public $foo = "";
}

$a = rand(0, 10) ? new A() : new B();
if (rand(0, 1)) {
    $a = null;
}
$b = null;

if (rand(0, 10) === 4) {
    // do nothing
}
elseif ($a instanceof A || $a instanceof B) {
    $b = $a->foo;
}
