<?php
class A {
    /** @return void */
    public function fooFoo() {}
}

class B {
    /** @var A|null */
    protected $a;

    /** @return void */
    public function barBar(A $a = null) {
        $this->a = $a;
        $this->a->fooFoo();
    }
}
