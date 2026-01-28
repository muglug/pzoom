<?php
interface I {
    function foo(int $s): void;
}

abstract class C implements I {
    public function foo(string $s): void {}
}
