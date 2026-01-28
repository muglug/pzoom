<?php
class A {
    /** @var ?string */
    public $foo;

    /** @var ?string */
    public $bar;
}

$a1 = rand(0, 1) ? new A() : null;
$a4 = rand(0, 1) ? new A() : null;
$a5 = rand(0, 1) ? new A() : null;
$a7 = rand(0, 1) ? new A() : null;
$a8 = rand(0, 1) ? new A() : null;

if ($a1 || (($a4 && $a5) || ($a7 && $a8))) {}