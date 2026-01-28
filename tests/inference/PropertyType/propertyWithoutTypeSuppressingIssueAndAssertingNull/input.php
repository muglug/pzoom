<?php
class A {
    /** @return void */
    function foo() {
        $boop = $this->foo === null && rand(0,1);

        echo $this->foo->baz;
    }
}
