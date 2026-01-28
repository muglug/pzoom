<?php
class A {
    /** @return void */
    public function fooFoo() {

    }
}

class B {
    /** @return void */
    public function barBar() {

    }
}

$a = rand(0, 10) ? new A() : new B();

switch (get_class($a)) {
    case A::class:
        $a->barBar();
        break;
}
