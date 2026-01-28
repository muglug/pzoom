<?php
class A {}

class B {
    /** @return void */
    public function barBar(A $a) {}
}

$b = new B();
$b->barBar(new A);