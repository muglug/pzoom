<?php
class A {
    /** @return void */
    public function fooFoo() {}
}

class B {
    /** @var A|null */
    public $a;

    /** @return void */
    public function barBar(A $a = null) {
        $this->a = $a;
        $b = $this->a ? $this->a->fooFoo() : null;
    }
}