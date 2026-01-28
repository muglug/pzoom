<?php
class A {
    /** @var A|null */
    public $a;

    public function __construct() {
        $this->a = rand(0, 1) ? new A : null;
    }
}

function takesA(A $a): void {}

$a = new A();
do {
    takesA($a);
    $a = $a->a;
} while ($a);
