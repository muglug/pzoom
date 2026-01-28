<?php
class A {
    /** @return void */
    public function fooFoo() {}
}

class B {
    /** @return void */
    public function barBar(A $a = null) {
        $b = is_null($a) ? null : $a->fooFoo();
    }
}