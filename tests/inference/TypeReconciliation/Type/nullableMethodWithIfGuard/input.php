<?php
class A {
    /** @return void */
    public function fooFoo() {}
}

class B {
    /** @return void */
    public function barBar(A $a = null) {
        if ($a) {
            $a->fooFoo();
        }
    }
}