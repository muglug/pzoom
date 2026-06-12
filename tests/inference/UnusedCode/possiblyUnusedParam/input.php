<?php
class A {
    /** @return void */
    public function foo(int $i) {}
}

(new A)->foo(4);
