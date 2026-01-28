<?php
class A {
    /** @return void */
    public function fooFoo() {}

    /** @return void */
    public function barFoo() {}
}

class B {
    /** @return void */
    public function barBar(A $a = null) {
        /** @psalm-suppress PossiblyNullReference */
        $a->fooFoo();

        $a->barFoo();
    }
}