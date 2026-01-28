<?php
interface A {
    /** @return ?string */
    function foo();
}

class C implements A {
    public function foo() : ?string {}
}
