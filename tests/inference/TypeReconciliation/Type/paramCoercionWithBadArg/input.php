<?php
class A {}
class B extends A {
    /** @return void */
    public function blab() {}
}

class C {
    /** @return void */
    function fooFoo(A $a) {
        if ($a instanceof B) {
            $a->barBar();
        }
    }
}
