<?php
class A {
    public function fooFoo(): void { }
}
function bar (array $a): void {
    if ($a[0] instanceof A) {
        $a[0]->fooFoo();
    }
}
