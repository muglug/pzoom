<?php
/**
 * @psalm-template TO of object
 */
interface A {
    /**
     * @psalm-param Closure(TO&A):mixed $c
     */
    public function setClosure(Closure $c): void;
}

function foo(A $i) : void {
    $i->setClosure(
        function(A $i) : string {
            return "hello";
        }
    );
}