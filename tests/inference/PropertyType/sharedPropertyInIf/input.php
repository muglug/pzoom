<?php
class A {
    /** @var int */
    public $foo = 0;
}
class B {
    /** @var string */
    public $foo = "";
}

$a = rand(0, 10) ? new A(): (rand(0, 10) ? new B() : null);
$b = null;

if ($a instanceof A || $a instanceof B) {
    $b = $a->foo;
}
