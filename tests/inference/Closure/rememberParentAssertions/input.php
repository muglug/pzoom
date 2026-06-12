<?php
class A {
    public ?A $a = null;
    public function foo() : void {}
}

function doFoo(A $a): void {
    if ($a->a instanceof A) {
        function () use ($a): void {
            $a->a->foo();
        };
    }
}
