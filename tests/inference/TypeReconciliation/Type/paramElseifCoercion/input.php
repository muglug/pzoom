<?php
class A {}
class B extends A {
    /** @return void */
    public function barBar() {}
}
class C extends A {
    /** @return void */
    public function baz() {}
}

class D {
    /** @return void */
    function fooFoo(A $a) {
        if ($a instanceof B) {
            $a->barBar();
        }
        elseif ($a instanceof C) {
            $a->baz();
        }
    }
}