<?php
class A {
    /** @var string|null */
    public $aa;
}

$a = new A();

if (!$a->aa) {
    $a->aa = "hello";
}

echo substr($a->aa, 1);
