<?php
class A {}
class B extends A {
    public function foo(): void {}
}

function takesA(A $a): void {
    $class = get_class($a);
    switch ($class) {
        case B::class:
            $a->foo();
            break;
    }
}
