<?php
interface A { }
interface B {
    function foo() : void;
}
function bar(A $a): void {
    if ($a instanceof B) {
        $a->foo();
    }
}
