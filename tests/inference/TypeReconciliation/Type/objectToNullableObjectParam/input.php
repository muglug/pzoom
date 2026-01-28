<?php
class A {}

class B {
    /** @return void */
    public function barBar(A $a = null) {}
}

$b = new B();
$b->barBar(new A);