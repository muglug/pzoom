<?php
class A {
    public function fooFoo(): void { }
}
function bar (array $a): void {
    if ($a["a"] instanceof A) {
        $a["a"]->fooFoo();
    }
}
