<?php
interface I {
    function foo(): array;
}

abstract class C implements I {
    public function foo(): void {}
}
